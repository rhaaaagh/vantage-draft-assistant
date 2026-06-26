use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use reqwest::blocking::{Client, ClientBuilder};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

fn log_lcu(msg: &str) {
    let path = crate::paths::log_path();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "[LCU] {}", msg);
        let _ = f.flush();
    }
}

use crate::{DraftSlot, DraftState, Phase, Role, TeamDraft, TeamSide};

/// Имя чемпиона по ID — из каталога Data Dragon (см. ddragon.rs).
pub fn champion_display_name(id: u32) -> String {
    crate::ddragon::display_name(id)
}

#[derive(Debug)]
pub struct LcuConnection {
    pub port: u16,
    pub token: String,
}

fn build_client() -> Result<Client, String> {
    ClientBuilder::new()
        .timeout(Duration::from_secs(2))
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())
}

/// Читаем порт и пароль из lockfile (путь: .../League of Legends/lockfile).
fn try_lockfile(league_dir: &Path) -> Option<LcuConnection> {
    let lock = league_dir.join("lockfile");
    let s = std::fs::read_to_string(&lock).ok()?;
    let parts: Vec<&str> = s.trim().split(':').collect();
    // формат: name:pid:port:password:protocol
    if parts.len() < 5 {
        return None;
    }
    let port: u16 = parts[2].parse().ok()?;
    let token = parts[3].to_string();
    if token.is_empty() {
        return None;
    }
    Some(LcuConnection { port, token })
}

/// Ищем процесс LeagueClientUx: сначала стандартные пути lockfile, потом командная строка процесса.
/// Всё обёрнуто в catch_unwind — перебор процессов (sysinfo) иногда падает или крашит на некоторых системах.
fn find_lcu_connection() -> Option<LcuConnection> {
    match std::panic::catch_unwind(find_lcu_connection_inner) {
        Ok(opt) => opt,
        Err(_) => {
            log_lcu("find_lcu_connection: panic/crash in find_lcu, returning None");
            None
        }
    }
}

/// Файл с пользовательским путём к League of Legends (та же папка, что и лог).
fn league_path_file() -> std::path::PathBuf {
    crate::paths::league_path_file()
}

/// Читает пользовательский путь к папке League из файла (если задан в настройках).
pub fn custom_league_path() -> Option<std::path::PathBuf> {
    let content = std::fs::read_to_string(league_path_file()).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(std::path::PathBuf::from(trimmed))
}

/// Записывает путь к League в файл (вызывается из настроек).
pub fn set_league_path(path: &str) -> Result<(), String> {
    let p = league_path_file();
    std::fs::write(&p, path.trim()).map_err(|e| e.to_string())
}

/// Возвращает сохранённый путь к League (для отображения в настройках).
pub fn get_league_path() -> String {
    std::fs::read_to_string(league_path_file())
        .ok()
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn find_lcu_connection_inner() -> Option<LcuConnection> {
    log_lcu("find_lcu: start");

    // 0) Путь из настроек (работает на любом ПК)
    if let Some(custom) = custom_league_path() {
        if let Some(conn) = try_lockfile(&custom) {
            log_lcu(&format!("find_lcu: found via custom path port {}", conn.port));
            return Some(conn);
        }
    }

    // 1) Стандартные пути к lockfile (без перебора процессов — стабильно)
    let mut paths: Vec<std::path::PathBuf> = vec![
        Path::new(r"C:\Riot Games\League of Legends").to_path_buf(),
        Path::new(r"D:\Riot Games\League of Legends").to_path_buf(),
        Path::new(r"C:\Program Files\Riot Games\League of Legends").to_path_buf(),
    ];
    if let Ok(pf) = std::env::var("ProgramFiles") {
        paths.push(Path::new(&pf).join("Riot Games").join("League of Legends"));
    }
    if let Ok(pf) = std::env::var("ProgramFiles(x86)") {
        paths.push(Path::new(&pf).join("Riot Games").join("League of Legends"));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        paths.push(Path::new(&local).join("Riot Games").join("League of Legends"));
    }
    for p in &paths {
        if let Some(conn) = try_lockfile(p) {
            log_lcu(&format!("find_lcu: found via lockfile port {}", conn.port));
            return Some(conn);
        }
    }
    // Поиск по списку процессов (sysinfo) отключён: на части систем он вызывает нативный краш
    // и бесконечный перезапуск приложения. Используется только lockfile.
    log_lcu("find_lcu: lockfile not found (only lockfile is used to avoid crashes)");
    None
}

fn auth_header(token: &str) -> String {
    let raw = format!("riot:{token}");
    let encoded = BASE64.encode(raw);
    format!("Basic {encoded}")
}

#[derive(Deserialize, Default)]
struct ChampSelectBans {
    #[serde(rename = "myTeamBans", default)]
    my_team_bans: Vec<i32>,
    #[serde(rename = "theirTeamBans", default)]
    their_team_bans: Vec<i32>,
}

#[derive(Deserialize, Clone)]
struct ChampSelectPlayer {
    #[serde(rename = "cellId", default)]
    cell_id: i32,
    #[serde(rename = "championId", default)]
    champion_id: i32,
    #[serde(rename = "championPickIntent", default)]
    champion_pick_intent: i32,
    #[serde(rename = "assignedPosition", default)]
    assigned_position: Option<String>,
    #[serde(rename = "summonerName", default)]
    summoner_name: Option<String>,
    #[serde(rename = "gameName", default)]
    game_name: Option<String>,
}

#[derive(Deserialize, Clone)]
struct ChampSelectAction {
    #[serde(rename = "actorCellId", default)]
    actor_cell_id: i32,
    #[serde(rename = "championId", default)]
    champion_id: i32,
    #[serde(rename = "completed", default)]
    #[allow(dead_code)] // часть формы LCU JSON; держим для полноты DTO
    completed: bool,
    #[serde(rename = "type", default)]
    action_type: String,
    #[serde(rename = "isAllyAction", default)]
    is_ally_action: bool,
}

#[derive(Deserialize)]
#[allow(dead_code)] // DTO формы champ-select из LCU; парсится, часть полей про запас
struct ChampSelectSession {
    #[serde(rename = "myTeam", default)]
    my_team: Vec<ChampSelectPlayer>,
    #[serde(rename = "theirTeam", default)]
    their_team: Vec<ChampSelectPlayer>,
    #[serde(default)]
    bans: ChampSelectBans,
    #[serde(rename = "actions", default)]
    actions: Option<Vec<Vec<ChampSelectAction>>>,
}

fn map_position_to_role(pos: &str) -> Role {
    match pos.to_ascii_uppercase().as_str() {
        "TOP" => Role::Top,
        "JUNGLE" => Role::Jungle,
        "MIDDLE" | "MID" => Role::Mid,
        "BOTTOM" | "ADC" | "BOT" => Role::Bot,
        "UTILITY" | "SUPPORT" => Role::Support,
        _ => Role::Mid,
    }
}

/// Создаёт до 5 слотов для команды. Игроки размещаются по порядку в массиве (индекс = слот).
fn map_players_to_team(players: Vec<ChampSelectPlayer>, side: TeamSide, _cell_offset: i32) -> TeamDraft {
    let mut slots: Vec<DraftSlot> = (0..5)
        .map(|_| DraftSlot {
            champion_id: None,
            champion_name: None,
            role: None,
            player_name: None,
        })
        .collect();

    for (idx, p) in players.iter().take(5).enumerate() {
        let role = p.assigned_position.as_deref().map(map_position_to_role);
        let champ = if p.champion_id > 0 {
            p.champion_id as u32
        } else if p.champion_pick_intent > 0 {
            p.champion_pick_intent as u32
        } else {
            0
        };
        slots[idx] = DraftSlot {
            champion_id: if champ > 0 { Some(champ) } else { None },
            champion_name: if champ > 0 { Some(champion_display_name(champ)) } else { None },
            role,
            player_name: p.summoner_name.clone().or(p.game_name.clone()),
        };
    }

    TeamDraft {
        side,
        slots,
        bans: Vec::new(),
    }
}

/// Собирает баны из actions (type "ban"). isAllyAction == true → баны моей команды (blue), false → баны противника (red).
fn apply_bans_from_actions(
    blue: &mut TeamDraft,
    red: &mut TeamDraft,
    actions: &[Vec<ChampSelectAction>],
) {
    for round in actions {
        for act in round {
            if !act.action_type.eq_ignore_ascii_case("ban") || act.champion_id <= 0 {
                continue;
            }
            let id = act.champion_id as u32;
            if act.is_ally_action {
                if !blue.bans.contains(&id) {
                    blue.bans.push(id);
                }
            } else if !red.bans.contains(&id) {
                red.bans.push(id);
            }
        }
    }
}

/// Заполняет пики из actions. Маппинг actorCellId -> слот по индексу в my_team/their_team (cellId совпадает).
fn apply_pick_actions(
    blue: &mut TeamDraft,
    red: &mut TeamDraft,
    actions: &[Vec<ChampSelectAction>],
    my_team: &[ChampSelectPlayer],
    their_team: &[ChampSelectPlayer],
) {
    for round in actions {
        for act in round {
            if !act.action_type.eq_ignore_ascii_case("pick") || act.champion_id <= 0 {
                continue;
            }
            let champ = act.champion_id as u32;
            let cell = act.actor_cell_id;

            if let Some((idx, _)) = my_team.iter().take(5).enumerate().find(|(_, p)| p.cell_id == cell) {
                if idx < blue.slots.len() {
                    blue.slots[idx].champion_id = Some(champ);
                    blue.slots[idx].champion_name = Some(champion_display_name(champ));
                }
            } else if let Some((idx, _)) = their_team.iter().take(5).enumerate().find(|(_, p)| p.cell_id == cell) {
                if idx < red.slots.len() {
                    red.slots[idx].champion_id = Some(champ);
                    red.slots[idx].champion_name = Some(champion_display_name(champ));
                }
            }
        }
    }
}

/// Парсит сессию из сырого JSON (Value). Не паникует — при ошибке возвращает None.
fn parse_session_from_value(raw: &serde_json::Value) -> Option<(Vec<ChampSelectPlayer>, Vec<ChampSelectPlayer>, ChampSelectBans, Option<Vec<Vec<ChampSelectAction>>>)> {
    let my_team: Vec<ChampSelectPlayer> = serde_json::from_value(raw.get("myTeam")?.clone()).ok()?;
    let their_team: Vec<ChampSelectPlayer> = serde_json::from_value(
        raw.get("theirTeam").cloned().unwrap_or(serde_json::json!([])),
    )
    .ok()?;
    let bans: ChampSelectBans = serde_json::from_value(raw.get("bans")?.clone()).ok()?;
    let actions: Option<Vec<Vec<ChampSelectAction>>> = raw
        .get("actions")
        .and_then(|a| serde_json::from_value(a.clone()).ok());
    Some((my_team, their_team, bans, actions))
}

/// Достаёт баны из сырого JSON actions (если типизированный парсинг не сработал или дал пусто).
fn apply_bans_from_raw_json(raw: &serde_json::Value, blue: &mut TeamDraft, red: &mut TeamDraft) {
    let Some(actions_arr) = raw.get("actions").and_then(|a| a.as_array()) else { return };
    for round in actions_arr {
        let Some(round_arr) = round.as_array() else { continue };
        for act in round_arr {
            let Some(obj) = act.as_object() else { continue };
            let ty = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if !ty.eq_ignore_ascii_case("ban") {
                continue;
            }
            let raw_id = obj.get("championId").and_then(|v| v.as_i64()).unwrap_or(0);
            if raw_id <= 0 {
                continue;
            }
            let champ_id = raw_id as u32;
            let is_ally = obj.get("isAllyAction").and_then(|v| v.as_bool()).unwrap_or(false);
            if is_ally {
                if !blue.bans.contains(&champ_id) {
                    blue.bans.push(champ_id);
                }
            } else if !red.bans.contains(&champ_id) {
                red.bans.push(champ_id);
            }
        }
    }
}

/// Безопасно собирает DraftState из сырого JSON. Не использует panic — при любой ошибке возвращает None.
fn safe_parse_session_to_draft(body: &str) -> Option<DraftState> {
    let raw: serde_json::Value = serde_json::from_str(body).ok()?;
    let (my_team, their_team, bans, actions) = parse_session_from_value(&raw)?;
    let mut blue = map_players_to_team(my_team.clone(), TeamSide::Blue, 0);
    let mut red = map_players_to_team(their_team.clone(), TeamSide::Red, 5);
    blue.bans = bans
        .my_team_bans
        .into_iter()
        .filter(|&id| id > 0)
        .map(|id| id as u32)
        .collect();
    red.bans = bans
        .their_team_bans
        .into_iter()
        .filter(|&id| id > 0)
        .map(|id| id as u32)
        .collect();
    if let Some(ref a) = actions {
        apply_pick_actions(&mut blue, &mut red, a, &my_team, &their_team);
        apply_bans_from_actions(&mut blue, &mut red, a);
    }
    // Запасной разбор банов из сырого JSON (на случай пустых типизированных actions)
    apply_bans_from_raw_json(&raw, &mut blue, &mut red);
    log_lcu(&format!(
        "bans after parse: blue={} {:?}, red={} {:?}",
        blue.bans.len(),
        blue.bans,
        red.bans.len(),
        red.bans
    ));
    Some(DraftState {
        phase: Phase::ChampSelect,
        blue,
        red,
    })
}

/// Определяет роль локального игрока по сырому JSON сессии:
/// читает `localPlayerCellId` и ищет в `myTeam` игрока с этим cellId.
/// Возвращает None, если cellId не задан, игрок не найден или у него нет роли.
fn parse_local_role_from_value(raw: &serde_json::Value) -> Option<Role> {
    let local_cell = raw.get("localPlayerCellId").and_then(|v| v.as_i64())?;
    if local_cell < 0 {
        return None;
    }
    let my_team = raw.get("myTeam").and_then(|v| v.as_array())?;
    for p in my_team {
        let cell = p.get("cellId").and_then(|v| v.as_i64());
        if cell == Some(local_cell) {
            let pos = p.get("assignedPosition").and_then(|v| v.as_str())?;
            if pos.trim().is_empty() {
                return None;
            }
            return Some(map_position_to_role(pos));
        }
    }
    None
}

/// Безопасно достаёт роль локального игрока из сырого тела сессии (без паник).
fn safe_parse_local_role(body: &str) -> Option<Role> {
    let raw: serde_json::Value = serde_json::from_str(body).ok()?;
    parse_local_role_from_value(&raw)
}

/// Пишет в lol_draft_summary.txt текстовую сводку: кто пикнут и кто забанен (с именами чемпионов).
fn write_draft_summary_file(draft: &DraftState) {
    let mut lines: Vec<String> = Vec::new();
    lines.push("=== ДРАФТ (пики и баны) ===".to_string());
    lines.push(String::new());

    let blue_picks: Vec<String> = draft
        .blue
        .slots
        .iter()
        .filter_map(|s| s.champion_id.map(champion_display_name))
        .collect();
    let red_picks: Vec<String> = draft
        .red
        .slots
        .iter()
        .filter_map(|s| s.champion_id.map(champion_display_name))
        .collect();
    let blue_bans: Vec<String> = draft.blue.bans.iter().map(|&id| champion_display_name(id)).collect();
    let red_bans: Vec<String> = draft.red.bans.iter().map(|&id| champion_display_name(id)).collect();

    lines.push("Твоя команда (Blue) — пики:".to_string());
    if blue_picks.is_empty() {
        lines.push("  (пока никого)".to_string());
    } else {
        for name in &blue_picks {
            lines.push(format!("  • {}", name));
        }
    }
    lines.push(String::new());
    lines.push("Твоя команда (Blue) — баны:".to_string());
    if blue_bans.is_empty() {
        lines.push("  (нет)".to_string());
    } else {
        for name in &blue_bans {
            lines.push(format!("  • {}", name));
        }
    }
    lines.push(String::new());
    lines.push("Команда противника (Red) — пики:".to_string());
    if red_picks.is_empty() {
        lines.push("  (пока никого)".to_string());
    } else {
        for name in &red_picks {
            lines.push(format!("  • {}", name));
        }
    }
    lines.push(String::new());
    lines.push("Команда противника (Red) — баны:".to_string());
    if red_bans.is_empty() {
        lines.push("  (нет)".to_string());
    } else {
        for name in &red_bans {
            lines.push(format!("  • {}", name));
        }
    }
    lines.push(String::new());
    lines.push("(Файл обновляется при проверке LCU или обновлении драфта.)".to_string());

    let content = lines.join("\n");
    let path = crate::paths::data_dir().join("lol_draft_summary.txt");
    if let Err(e) = std::fs::write(&path, &content) {
        log_lcu(&format!("write_draft_summary_file: failed to write: {}", e));
    } else {
        log_lcu(&format!("write_draft_summary_file: written to {:?}", path));
    }
}

/// Пытаемся получить состояние champ select из LCU.
pub fn fetch_champ_select_state() -> Option<DraftState> {
    log_lcu("fetch_champ_select_state: start");
    let conn = match find_lcu_connection() {
        Some(c) => {
            log_lcu(&format!("fetch_champ_select_state: LCU found port {}", c.port));
            c
        }
        None => {
            log_lcu("fetch_champ_select_state: LCU not found");
            return None;
        }
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            log_lcu(&format!("fetch_champ_select_state: build_client err: {}", e));
            return None;
        }
    };
    let url = format!("https://127.0.0.1:{}/lol-champ-select/v1/session", conn.port);

    let res = match client
        .get(&url)
        .header(AUTHORIZATION, auth_header(&conn.token))
        .header(CONTENT_TYPE, "application/json")
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            log_lcu(&format!("fetch_champ_select_state: send err: {}", e));
            return None;
        }
    };

    if !res.status().is_success() {
        log_lcu(&format!("fetch_champ_select_state: HTTP {}", res.status()));
        return None;
    }

    let body = match res.text() {
        Ok(b) => b,
        Err(e) => {
            log_lcu(&format!("fetch_champ_select_state: text err: {}", e));
            return None;
        }
    };
    log_lcu(&format!("fetch_champ_select_state: body len {}", body.len()));

    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| safe_parse_session_to_draft(&body))) {
        Ok(Some(draft)) => {
            log_lcu("fetch_champ_select_state: parse OK");
            let blue_picks: Vec<u32> = draft.blue.slots.iter().filter_map(|s| s.champion_id).collect();
            let red_picks: Vec<u32> = draft.red.slots.iter().filter_map(|s| s.champion_id).collect();
            log_lcu(&format!("Пики твоей команды (Blue): {:?}", blue_picks));
            log_lcu(&format!("Пики противника (Red): {:?}", red_picks));
            log_lcu(&format!("Баны Blue: {:?}", draft.blue.bans));
            log_lcu(&format!("Баны Red: {:?}", draft.red.bans));
            log_lcu(&format!("bans_blue={:?} bans_red={:?}", draft.blue.bans, draft.red.bans));
            Some(draft)
        }
        Ok(None) => {
            log_lcu("fetch_champ_select_state: parse returned None");
            None
        }
        Err(_) => {
            log_lcu("fetch_champ_select_state: parse PANIC");
            None
        }
    }
}

/// Получает состояние champ select ВМЕСТЕ с авто-определённой ролью локального игрока.
/// Делает один HTTP-запрос. Возвращает None, если LCU не найден / не в чемпион-селекте /
/// ответ не распарсился. Роль может быть None, даже если драфт есть (роль ещё не назначена).
pub fn fetch_champ_select_live() -> Option<(DraftState, Option<Role>)> {
    let conn = match find_lcu_connection() {
        Some(c) => c,
        None => {
            log_lcu("fetch_champ_select_live: LCU not found");
            return None;
        }
    };
    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            log_lcu(&format!("fetch_champ_select_live: build_client err: {}", e));
            return None;
        }
    };
    let url = format!("https://127.0.0.1:{}/lol-champ-select/v1/session", conn.port);
    let res = match client
        .get(&url)
        .header(AUTHORIZATION, auth_header(&conn.token))
        .header(CONTENT_TYPE, "application/json")
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            log_lcu(&format!("fetch_champ_select_live: send err: {}", e));
            return None;
        }
    };
    if !res.status().is_success() {
        log_lcu(&format!("fetch_champ_select_live: HTTP {}", res.status()));
        return None;
    }
    let body = match res.text() {
        Ok(b) => b,
        Err(e) => {
            log_lcu(&format!("fetch_champ_select_live: text err: {}", e));
            return None;
        }
    };

    let parsed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let draft = safe_parse_session_to_draft(&body)?;
        let role = safe_parse_local_role(&body);
        Some((draft, role))
    }));
    match parsed {
        Ok(Some(pair)) => {
            log_lcu(&format!("fetch_champ_select_live: parse OK, role={:?}", pair.1));
            Some(pair)
        }
        Ok(None) => {
            log_lcu("fetch_champ_select_live: parse returned None");
            None
        }
        Err(_) => {
            log_lcu("fetch_champ_select_live: parse PANIC");
            None
        }
    }
}

/// Текущий авторизованный суммонир из LCU (для запроса активной игры через Riot API).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentSummoner {
    pub summoner_id: String,
    pub puuid: String,
}

/// Находит LCU и возвращает текущего суммонира (кто залогинен в клиенте).
pub fn get_current_summoner() -> Option<CurrentSummoner> {
    let conn = find_lcu_connection()?;
    fetch_current_summoner(&conn)
}

/// Запрашивает текущего суммонира из LCU (кто залогинен в клиенте).
pub fn fetch_current_summoner(conn: &LcuConnection) -> Option<CurrentSummoner> {
    let client = build_client().ok()?;
    let url = format!("https://127.0.0.1:{}/lol-summoner/v1/current-summoner", conn.port);
    let res = client
        .get(&url)
        .header(AUTHORIZATION, auth_header(&conn.token))
        .header(CONTENT_TYPE, "application/json")
        .send()
        .ok()?;
    if !res.status().is_success() {
        log_lcu(&format!("fetch_current_summoner: HTTP {}", res.status()));
        return None;
    }
    #[derive(serde::Deserialize)]
    struct SummonerPayload {
        #[serde(rename = "summonerId")]
        summoner_id: Option<serde_json::Value>,
        #[serde(rename = "puuid")]
        puuid: Option<String>,
    }
    let body: SummonerPayload = res.json().ok()?;
    let summoner_id = match &body.summoner_id {
        Some(serde_json::Value::Number(n)) => n.as_i64().map(|i| i.to_string()),
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        _ => None,
    }?;
    let puuid = body.puuid?;
    if puuid.is_empty() {
        return None;
    }
    Some(CurrentSummoner { summoner_id, puuid })
}

/// Платформа (хост Riot API, напр. "euw1", "ru", "eun1") залогиненного клиента — из LCU.
///
/// Нужна, чтобы Spectator-V5 и ранги шли на ВЕРНЫЙ регион аккаунта, а не на
/// регион из ручных Настроек. Если они не совпадают, Riot не может расшифровать
/// puuid на чужом шарде и возвращает HTTP 400 («Exception decrypting PUUID»).
pub fn get_current_platform() -> Option<String> {
    let conn = find_lcu_connection()?;
    fetch_platform_id(&conn)
}

/// Запрашивает платформу клиента: сперва прямой `platformId` (это и есть хост
/// Riot API в нижнем регистре), затем фолбэк на `region-locale` с маппингом.
pub fn fetch_platform_id(conn: &LcuConnection) -> Option<String> {
    let client = build_client().ok()?;
    let get_json = |path: &str| -> Option<serde_json::Value> {
        let url = format!("https://127.0.0.1:{}{}", conn.port, path);
        let res = client
            .get(&url)
            .header(AUTHORIZATION, auth_header(&conn.token))
            .header(CONTENT_TYPE, "application/json")
            .send()
            .ok()?;
        if !res.status().is_success() {
            return None;
        }
        res.json::<serde_json::Value>().ok()
    };

    // 1) platformId напрямую — уже совпадает с хостом Riot API (EUW1 → euw1).
    if let Some(v) = get_json("/lol-platform-config/v1/namespaces/LoginDataPacket") {
        if let Some(pid) = v.get("platformId").and_then(|x| x.as_str()) {
            let pid = pid.trim();
            if !pid.is_empty() {
                return Some(pid.to_lowercase());
            }
        }
    }
    // 2) Фолбэк: region-locale + маппинг кода региона на платформу.
    if let Some(v) = get_json("/riotclient/region-locale") {
        if let Some(region) = v.get("region").and_then(|x| x.as_str()) {
            if let Some(p) = region_code_to_platform(region) {
                return Some(p);
            }
        }
    }
    None
}

/// Код региона из LCU (EUW / RU / LAN…) или уже-платформа (EUW1) → хост Riot API.
fn region_code_to_platform(code: &str) -> Option<String> {
    let c = code.trim().to_uppercase();
    let p = match c.as_str() {
        "NA" | "NA1" => "na1",
        "EUW" | "EUW1" => "euw1",
        "EUNE" | "EUN" | "EUN1" => "eun1",
        "KR" => "kr",
        "RU" | "RU1" => "ru",
        "BR" | "BR1" => "br1",
        "JP" | "JP1" => "jp1",
        "LAN" | "LA1" => "la1",
        "LAS" | "LA2" => "la2",
        "OCE" | "OC1" => "oc1",
        "TR" | "TR1" => "tr1",
        "TW" | "TW2" => "tw2",
        "VN" | "VN2" => "vn2",
        "PH" | "PH2" => "ph2",
        "SG" | "SG2" => "sg2",
        "TH" | "TH2" => "th2",
        "ME" | "ME1" => "me1",
        // Уже выглядит как платформа (буквы+цифра) — берём как есть.
        other if other.chars().any(|ch| ch.is_ascii_digit()) => return Some(other.to_lowercase()),
        _ => "",
    };
    if p.is_empty() {
        None
    } else {
        Some(p.to_string())
    }
}

/// Результат проверки подключения к LCU (для UI).
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LcuCheckResult {
    pub found: bool,
    pub port: Option<u16>,
    pub message: String,
    pub session_saved: bool,
}

/// Проверяет, находим ли мы клиент и получаем ли ответ session. Сохраняет сырой JSON в lcu_session_debug.json.
pub fn check_lcu_and_fetch_session() -> LcuCheckResult {
    let conn = match find_lcu_connection() {
        Some(c) => c,
        None => {
            return LcuCheckResult {
                found: false,
                port: None,
                message: "LCU не найден: процесс League Client не обнаружен или lockfile недоступен.".into(),
                session_saved: false,
            };
        }
    };

    let client = match build_client() {
        Ok(c) => c,
        Err(e) => {
            return LcuCheckResult {
                found: false,
                port: None,
                message: format!("Ошибка инициализации: {}", e),
                session_saved: false,
            };
        }
    };
    let url = format!("https://127.0.0.1:{}/lol-champ-select/v1/session", conn.port);

    let res = match client
        .get(&url)
        .header(AUTHORIZATION, auth_header(&conn.token))
        .header(CONTENT_TYPE, "application/json")
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            return LcuCheckResult {
                found: true,
                port: Some(conn.port),
                message: format!("Подключение к порту {} есть, но запрос session failed: {}", conn.port, e),
                session_saved: false,
            };
        }
    };

    let status = res.status();
    let body = match res.text() {
        Ok(b) => b,
        Err(e) => {
            return LcuCheckResult {
                found: true,
                port: Some(conn.port),
                message: format!("Ответ получен, но не удалось прочитать тело: {}", e),
                session_saved: false,
            };
        }
    };

    let session_saved = if status.is_success() {
        // Отладочные файлы пишем только из явной проверки LCU (не из горячего пути драфта).
        if let Some(draft) = safe_parse_session_to_draft(&body) {
            write_draft_summary_file(&draft);
        }
        std::fs::write(crate::paths::lcu_debug_file(), &body).is_ok()
    } else {
        false
    };

    let message = if status.is_success() {
        if session_saved {
            format!(
                "LCU найден, порт {}. Сессия получена ({} байт). Сохранено в lcu_session_debug.json — открой файл и пришли его содержимое, если драфт в приложении не отображается.",
                conn.port,
                body.len()
            )
        } else {
            format!("LCU найден, порт {}. Сессия получена ({} байт).", conn.port, body.len())
        }
    } else {
        format!(
            "LCU найден, порт {}. Запрос session вернул HTTP {} (не champ select или ошибка).",
            conn.port,
            status.as_u16()
        )
    };

    LcuCheckResult {
        found: true,
        port: Some(conn.port),
        message,
        session_saved,
    }
}

