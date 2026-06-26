use crate::db::Database;
use crate::riot_api::{
    self, fetch_apex_seeds, fetch_division_seeds, fetch_match, fetch_match_ids,
    fetch_match_timeline, fetch_puuid_by_summoner_id, RiotConfig, RiotError, SharedLimiter,
};
use reqwest::blocking::Client;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Статус краулера для UI (поллится с фронта).
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrawlStatus {
    pub running: bool,
    pub seeding: bool,
    pub puuids_total: i64,
    pub puuids_done: i64,
    pub matches_total: i64,
    pub matches_done: i64,
    pub target: u32,
    pub last_error: Option<String>,
    pub message: String,
}

impl Default for CrawlStatus {
    fn default() -> Self {
        Self {
            running: false,
            seeding: false,
            puuids_total: 0,
            puuids_done: 0,
            matches_total: 0,
            matches_done: 0,
            target: 0,
            last_error: None,
            message: "Краулер не запущен.".into(),
        }
    }
}

pub struct CrawlControl {
    pub stop: Arc<AtomicBool>,
    pub status: Arc<Mutex<CrawlStatus>>,
}

impl CrawlControl {
    pub fn new() -> Self {
        Self {
            stop: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(CrawlStatus::default())),
        }
    }
}

fn set_msg(status: &Arc<Mutex<CrawlStatus>>, msg: &str) {
    if let Ok(mut s) = status.lock() {
        s.message = msg.to_string();
    }
}

fn refresh_counts(db: &Database, status: &Arc<Mutex<CrawlStatus>>) {
    if let Ok((pt, pd, mt, md)) = db.crawl_counts() {
        if let Ok(mut s) = status.lock() {
            s.puuids_total = pt;
            s.puuids_done = pd;
            s.matches_total = mt;
            s.matches_done = md;
        }
    }
}

/// TOP/JUNGLE/MIDDLE/BOTTOM/UTILITY → TOP/JUNGLE/MID/BOT/SUPPORT. Пусто/Invalid → None.
fn normalize_position(pos: &str) -> Option<&'static str> {
    match pos.to_ascii_uppercase().as_str() {
        "TOP" => Some("TOP"),
        "JUNGLE" => Some("JUNGLE"),
        "MIDDLE" | "MID" => Some("MID"),
        "BOTTOM" | "BOT" => Some("BOT"),
        "UTILITY" | "SUPPORT" => Some("SUPPORT"),
        _ => None,
    }
}

/// Главная точка: запускается в отдельном потоке.
/// current_patch (например "16.12") — собираются только матчи этого патча; None = без фильтра.
pub fn run_crawl(
    api_key: String,
    region: String,
    include_diamond: bool,
    max_matches: u32,
    reset: bool,
    current_patch: Option<String>,
    limiter: SharedLimiter,
    stop: Arc<AtomicBool>,
    status: Arc<Mutex<CrawlStatus>>,
) {
    if let Ok(mut s) = status.lock() {
        s.running = true;
        s.last_error = None;
        s.target = max_matches;
        s.message = "Запуск…".into();
    }

    let result = run_inner(
        &api_key,
        &region,
        include_diamond,
        max_matches,
        reset,
        current_patch.as_deref(),
        &limiter,
        &stop,
        &status,
    );

    if let Ok(mut s) = status.lock() {
        s.running = false;
        s.seeding = false;
        match result {
            Ok(stopped) => {
                s.message = if stopped {
                    "Остановлено. Можно продолжить — прогресс сохранён.".into()
                } else {
                    "Готово: цель по матчам достигнута.".into()
                };
            }
            Err(e) => {
                s.message = format!("Остановлено с ошибкой: {}", e);
                s.last_error = Some(e);
            }
        }
    }
}

/// Возвращает Ok(true) если остановлено пользователем, Ok(false) если достигнута цель.
fn run_inner(
    api_key: &str,
    region: &str,
    include_diamond: bool,
    max_matches: u32,
    reset: bool,
    current_patch: Option<&str>,
    limiter: &SharedLimiter,
    stop: &Arc<AtomicBool>,
    status: &Arc<Mutex<CrawlStatus>>,
) -> Result<bool, String> {
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let cfg = RiotConfig {
        api_key: api_key.to_string(),
        region: region.to_string(),
    };
    let db = Database::open(&crate::paths::db_path())?;

    if reset {
        set_msg(status, "Сброс старой базы…");
        let _ = db.reset_crawl_data();
    }

    // Чистим агрегаты старых патчей — оставляем текущий и один предыдущий
    // (предыдущий нужен, чтобы сразу после выхода патча приложению было что показать).
    if let Some(p) = current_patch {
        let _ = db.purge_keep_current_and_prev(p);
    }

    // Множество завершённых предметов (для определения первого собранного) — один раз.
    let (completed_items, boots_items): (HashSet<i32>, HashSet<i32>) =
        match crate::ddragon::current_version() {
            Some(ver) => (
                crate::ddragon::fetch_completed_items(&client, &ver),
                crate::ddragon::fetch_boots_items(&client, &ver),
            ),
            None => (HashSet::new(), HashSet::new()),
        };

    // 1) Сидинг (только если пусто — иначе продолжаем прошлый прогон).
    if !db.has_seeds()? {
        set_msg(status, "Сбор игроков-сидов (Challenger/GM/Master…)");
        if let Ok(mut s) = status.lock() {
            s.seeding = true;
        }
        seed(&client, &cfg, include_diamond, limiter, stop, status, &db)?;
        if let Ok(mut s) = status.lock() {
            s.seeding = false;
        }
        if stop.load(Ordering::Relaxed) {
            return Ok(true);
        }
    }
    refresh_counts(&db, status);

    // Если задан патч — тянем у игроков только матчи за временно́е окно текущего патча
    // (серверный фильтр startTime). Патч Riot живёт ~2 недели; берём с запасом, чтобы
    // не потерять матчи в начале патча. Точную дату старта патча Riot по API не отдаёт,
    // поэтому хвост предыдущего патча (несколько дней) добивает дешёвая проверка
    // `wrong_patch` в process_match — но это уже единицы матчей, а не сотни.
    const PATCH_WINDOW_DAYS: i64 = 16;
    let match_ids_start: Option<i64> = current_patch.map(|_| {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        now - PATCH_WINDOW_DAYS * 24 * 60 * 60
    });

    // 2) Основной цикл.
    // Число готовых матчей держим в памяти: один раз читаем из базы, дальше +1 на
    // каждый записанный матч — чтобы не пересчитывать всю базу перед каждым матчем.
    // Раз в 10 матчей сверяемся с базой (страховка). Дедуп от этого не зависит:
    // он держится на PRIMARY KEY match_id + статусе строки, а не на счётчике.
    let mut since_refresh = 0u32;
    let (_, _, _, mut done_count) = db.crawl_counts()?;
    loop {
        if stop.load(Ordering::Relaxed) {
            return Ok(true);
        }
        if done_count >= max_matches as i64 {
            return Ok(false);
        }

        // Сначала разгребаем матчи в очереди.
        if let Some(match_id) = db.next_pending_match()? {
            match process_match(&client, &cfg, limiter, &db, &match_id, current_patch, &completed_items, &boots_items) {
                Ok(true) => done_count += 1, // матч записан
                Ok(false) => {}              // пропущен (не тот патч / ремейк)
                Err(RiotError::Unauthorized) => {
                    return Err(riot_api::RiotError::Unauthorized.to_user_message());
                }
                Err(RiotError::Forbidden) => {
                    return Err(riot_api::RiotError::Forbidden.to_user_message());
                }
                Err(_) => {
                    let _ = db.mark_match(&match_id, "error");
                }
            }
            since_refresh += 1;
            if since_refresh >= 10 {
                refresh_counts(&db, status);
                set_msg(status, "Сбор матчей…");
                // Сверяем локальный счётчик с базой (страховка от расхождений).
                if let Ok((_, _, _, md)) = db.crawl_counts() {
                    done_count = md;
                }
                since_refresh = 0;
            }
            continue;
        }

        // Матчи кончились — берём id матчей у следующего игрока.
        if let Some((puuid, _tier)) = db.next_pending_puuid()? {
            match fetch_match_ids(&client, &cfg, limiter, &puuid, 100, match_ids_start) {
                Ok(ids) => {
                    for id in ids {
                        let _ = db.add_crawl_match(&id);
                    }
                }
                Err(RiotError::Unauthorized) => return Err(RiotError::Unauthorized.to_user_message()),
                Err(RiotError::Forbidden) => return Err(RiotError::Forbidden.to_user_message()),
                Err(_) => {}
            }
            let _ = db.mark_puuid_done(&puuid);
            refresh_counts(&db, status);
            continue;
        }

        // Ни матчей, ни игроков — база сидов исчерпана.
        refresh_counts(&db, status);
        return Ok(false);
    }
}

fn seed(
    client: &Client,
    cfg: &RiotConfig,
    include_diamond: bool,
    limiter: &SharedLimiter,
    stop: &Arc<AtomicBool>,
    status: &Arc<Mutex<CrawlStatus>>,
    db: &Database,
) -> Result<(), String> {
    let apex = [
        ("challengerleagues", "CHALLENGER"),
        ("grandmasterleagues", "GRANDMASTER"),
        ("masterleagues", "MASTER"),
    ];
    for (path, tier) in apex {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        set_msg(status, &format!("Сиды: {}", tier));
        let seeds = fetch_apex_seeds(client, cfg, limiter, path).map_err(|e| e.to_user_message())?;
        add_seeds(client, cfg, limiter, db, &seeds, tier)?;
        refresh_counts(db, status);
    }

    if include_diamond {
        // Ограничиваем число страниц на дивизион, чтобы сидов было достаточно, но не бесконечно.
        const MAX_PAGES: u32 = 4;
        for division in ["I", "II", "III", "IV"] {
            for page in 1..=MAX_PAGES {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                set_msg(status, &format!("Сиды: DIAMOND {} стр.{}", division, page));
                let seeds = fetch_division_seeds(client, cfg, limiter, "DIAMOND", division, page)
                    .map_err(|e| e.to_user_message())?;
                if seeds.is_empty() {
                    break;
                }
                add_seeds(client, cfg, limiter, db, &seeds, "DIAMOND")?;
                refresh_counts(db, status);
            }
        }
    }
    Ok(())
}

fn add_seeds(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    db: &Database,
    seeds: &[riot_api::LeagueSeed],
    tier: &str,
) -> Result<(), String> {
    for seed in seeds {
        let puuid = if !seed.puuid.is_empty() {
            seed.puuid.clone()
        } else if !seed.summoner_id.is_empty() {
            // Лига не отдала puuid — конвертируем из summonerId.
            match fetch_puuid_by_summoner_id(client, cfg, limiter, &seed.summoner_id) {
                Ok(p) => p,
                Err(_) => continue,
            }
        } else {
            continue;
        };
        let _ = db.add_crawl_puuid(&puuid, tier);
    }
    Ok(())
}

fn process_match(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    db: &Database,
    match_id: &str,
    current_patch: Option<&str>,
    completed_items: &HashSet<i32>,
    boots_items: &HashSet<i32>,
) -> Result<bool, RiotError> {
    let parsed = fetch_match(client, cfg, limiter, match_id)?;
    // Только ранкед соло, не ремейки, и (если задан) только текущий патч.
    let wrong_patch = current_patch.map(|p| parsed.patch != p).unwrap_or(false);
    if parsed.queue_id != 420 || parsed.game_duration < 300 || wrong_patch {
        let _ = db.mark_match(match_id, "skip");
        return Ok(false); // пропущен — в счётчик готовых не идёт
    }

    // Таймлайн → первый завершённый предмет, ботинки и порядок предметов каждого игрока.
    #[derive(Default)]
    struct Build {
        first: i32,
        boots: i32,
        ordered: Vec<i32>,
    }
    let builds: HashMap<String, Build> = if completed_items.is_empty() {
        HashMap::new()
    } else {
        match fetch_match_timeline(client, cfg, limiter, match_id) {
            Ok(purchases) => {
                let mut map: HashMap<String, Build> = HashMap::new();
                for (puuid, item) in purchases {
                    let b = map.entry(puuid).or_default();
                    let is_completed = completed_items.contains(&item);
                    // Первый завершённый (может быть ботинками) — как раньше.
                    if is_completed && b.first == 0 {
                        b.first = item;
                    }
                    if boots_items.contains(&item) {
                        if b.boots == 0 {
                            b.boots = item;
                        }
                        continue; // ботинки не кладём в порядок предметов
                    }
                    if is_completed && !b.ordered.contains(&item) && b.ordered.len() < 6 {
                        b.ordered.push(item);
                    }
                }
                map
            }
            Err(_) => HashMap::new(),
        }
    };

    let players: Vec<crate::db::CrawlPlayer> = parsed
        .participants
        .iter()
        .map(|p| {
            let b = builds.get(&p.puuid);
            crate::db::CrawlPlayer {
                puuid: p.puuid.clone(),
                champion_id: p.champion_id,
                team_id: p.team_id,
                role: normalize_position(&p.team_position).unwrap_or("").to_string(),
                win: p.win,
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
                cs: p.total_minions_killed + p.neutral_minions_killed,
                items: [p.item0, p.item1, p.item2, p.item3, p.item4, p.item5, p.item6],
                first_item: b.map(|x| x.first).unwrap_or(0),
                boots: b.map(|x| x.boots).unwrap_or(0),
                ordered_items: b.map(|x| x.ordered.clone()).unwrap_or_default(),
                keystone_id: p.keystone_id(),
                primary_style_id: p.primary_style_id(),
                sub_style_id: p.sub_style_id(),
            }
        })
        .collect();

    let cm = crate::db::CrawlMatch {
        match_id: parsed.match_id.clone(),
        patch: parsed.patch.clone(),
        queue_id: parsed.queue_id,
        duration: parsed.game_duration,
        players,
        bans: parsed.bans.clone(),
    };

    db.record_match(&cm).map_err(RiotError::Network)?;
    let _ = db.mark_match(match_id, "done");
    Ok(true) // матч записан
}
