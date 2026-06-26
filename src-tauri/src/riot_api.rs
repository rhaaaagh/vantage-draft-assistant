use crate::rate_limit::{Priority, RateLimiter};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RiotConfig {
    pub api_key: String,
    pub region: String, // платформа, например "ru", "euw1"
}

/// Общий лимитер запросов с приоритетом. Клонируется дёшево (Arc внутри),
/// поэтому существующие `state.limiter.clone()` и `&SharedLimiter` работают как раньше.
/// Интерактивные команды используют High (по умолчанию), фоновый краулер — `.low()`.
#[derive(Clone)]
pub struct SharedLimiter {
    inner: Arc<RateLimiter>,
    priority: Priority,
}

impl SharedLimiter {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RateLimiter::new()),
            priority: Priority::High,
        }
    }

    /// Тот же лимитер, но с низким приоритетом — для фонового краулера.
    pub fn low(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            priority: Priority::Low,
        }
    }

    fn acquire(&self) {
        self.inner.acquire(self.priority);
    }
}

impl Default for SharedLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ---------- Ошибки ----------

#[derive(Debug)]
pub enum RiotError {
    Unauthorized,
    Forbidden,
    RateLimited { retry_after_secs: u64 },
    NotFound,
    Http(u16),
    Network(String),
}

impl RiotError {
    pub fn to_user_message(&self) -> String {
        match self {
            RiotError::Unauthorized => {
                "401: неверный или истёкший Riot API ключ. Development-ключ живёт 24 часа — получите новый на developer.riotgames.com и вставьте в Настройках.".into()
            }
            RiotError::Forbidden => {
                "403: ключ не имеет доступа к этому API. На developer.riotgames.com откройте своё приложение, включите продукты LoL (Summoner, Match, Spectator, League) и создайте новый ключ.".into()
            }
            RiotError::RateLimited { retry_after_secs } => format!(
                "429: превышен лимит запросов Riot API. Повторите через {} с.",
                retry_after_secs
            ),
            RiotError::NotFound => "404: данные не найдены.".into(),
            RiotError::Http(code) => format!("Riot API вернул HTTP {}.", code),
            RiotError::Network(e) => format!("Сетевая ошибка: {}", e),
        }
    }
}

impl From<RiotError> for String {
    fn from(e: RiotError) -> Self {
        e.to_user_message()
    }
}

// ---------- Маршрутизация регионов ----------

/// Платформа → кластер для account-v1 (только americas/asia/europe).
pub fn platform_to_account_region(platform: &str) -> &'static str {
    match platform {
        "euw1" | "eun1" | "tr1" | "ru" | "me1" => "europe",
        "na1" | "br1" | "la1" | "la2" => "americas",
        "kr" | "jp1" | "oc1" | "ph2" | "sg2" | "th2" | "tw2" | "vn2" => "asia",
        _ => "europe",
    }
}

/// Платформа → кластер для match-v5 (SEA-платформы идут на sea).
pub fn platform_to_match_region(platform: &str) -> &'static str {
    match platform {
        "euw1" | "eun1" | "tr1" | "ru" | "me1" => "europe",
        "na1" | "br1" | "la1" | "la2" => "americas",
        "kr" | "jp1" => "asia",
        "oc1" | "ph2" | "sg2" | "th2" | "tw2" | "vn2" => "sea",
        _ => "europe",
    }
}

// ---------- Общий GET с лимитером и обработкой статусов ----------

fn riot_get(
    client: &Client,
    limiter: &SharedLimiter,
    url: &str,
    api_key: &str,
) -> Result<reqwest::blocking::Response, RiotError> {
    let mut attempt = 0;
    loop {
        limiter.acquire();
        let res = client
            .get(url)
            .header("X-Riot-Token", api_key.trim())
            .send()
            .map_err(|e| RiotError::Network(e.to_string()))?;

        let status = res.status().as_u16();
        match status {
            200..=299 => return Ok(res),
            401 => return Err(RiotError::Unauthorized),
            403 => return Err(RiotError::Forbidden),
            404 => return Err(RiotError::NotFound),
            429 => {
                let retry_after = res
                    .headers()
                    .get("Retry-After")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(2)
                    .min(65);
                if attempt == 0 {
                    std::thread::sleep(Duration::from_secs(retry_after));
                    attempt += 1;
                    continue;
                }
                return Err(RiotError::RateLimited {
                    retry_after_secs: retry_after,
                });
            }
            code => return Err(RiotError::Http(code)),
        }
    }
}

// ---------- Account-V1 (Riot ID → PUUID) ----------

#[derive(Debug, Clone, Deserialize)]
pub struct RiotAccount {
    pub puuid: String,
    #[serde(rename = "gameName", default, deserialize_with = "de_nullable_string")]
    pub game_name: String,
    #[serde(rename = "tagLine", default, deserialize_with = "de_nullable_string")]
    pub tag_line: String,
}

/// Riot ID (Имя#TAG) → аккаунт с PUUID. Хост — кластерный (europe/americas/asia),
/// НЕ платформенный — на платформенном хосте account-v1 возвращает 404.
pub fn fetch_account_by_riot_id(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    game_name: &str,
    tag_line: &str,
) -> Result<RiotAccount, RiotError> {
    let cluster = platform_to_account_region(cfg.region.trim());
    let url = format!(
        "https://{}.api.riotgames.com/riot/account/v1/accounts/by-riot-id/{}/{}",
        cluster,
        urlencoding::encode(game_name.trim()),
        urlencoding::encode(tag_line.trim())
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    res.json().map_err(|e| RiotError::Network(e.to_string()))
}

// ---------- League-V4 (ранги, by-puuid) ----------

#[derive(Deserialize)]
struct LeagueEntryDto {
    #[serde(rename = "queueType")]
    queue_type: String,
    tier: String,
    rank: Option<String>,
    #[serde(rename = "leaguePoints", default)]
    league_points: i32,
    #[serde(default)]
    wins: i32,
    #[serde(default)]
    losses: i32,
}

/// Полная инфа о ранге соло-очереди (для скаута).
#[derive(Debug, Clone)]
pub struct LeagueRankInfo {
    pub tier: String,
    pub rank: String,
    pub league_points: i32,
    pub wins: i32,
    pub losses: i32,
}

/// Ранг соло-очереди RANKED_SOLO_5x5 (None = анранкед).
pub fn fetch_league_rank(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
) -> Result<Option<LeagueRankInfo>, RiotError> {
    let entries = fetch_league_entries(client, cfg, limiter, puuid)?;
    for e in entries {
        if e.queue_type == "RANKED_SOLO_5x5" {
            return Ok(Some(LeagueRankInfo {
                tier: e.tier,
                rank: e.rank.unwrap_or_default(),
                league_points: e.league_points,
                wins: e.wins,
                losses: e.losses,
            }));
        }
    }
    Ok(None)
}

fn fetch_league_entries(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
) -> Result<Vec<LeagueEntryDto>, RiotError> {
    let url = format!(
        "https://{}.api.riotgames.com/lol/league/v4/entries/by-puuid/{}",
        cfg.region.trim(),
        puuid
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    res.json().map_err(|e| RiotError::Network(e.to_string()))
}

fn is_emerald_or_higher(tier: &str) -> bool {
    matches!(
        tier.to_ascii_uppercase().as_str(),
        "EMERALD" | "DIAMOND" | "MASTER" | "GRANDMASTER" | "CHALLENGER"
    )
}

pub fn fetch_is_emerald_plus(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
) -> Result<bool, RiotError> {
    let entries = fetch_league_entries(client, cfg, limiter, puuid)?;
    for e in entries {
        if e.queue_type == "RANKED_SOLO_5x5" {
            return Ok(is_emerald_or_higher(&e.tier));
        }
    }
    Ok(false)
}

/// Строка ранга для отображения (например "EMERALD II") или "Unranked".
pub fn fetch_league_entry_display(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
) -> Result<String, RiotError> {
    let entries = fetch_league_entries(client, cfg, limiter, puuid)?;
    for e in entries {
        if e.queue_type == "RANKED_SOLO_5x5" {
            let rank = e.rank.as_deref().unwrap_or("");
            return Ok(format!("{} {}", e.tier, rank).trim().to_string());
        }
    }
    Ok("Unranked".to_string())
}

// ---------- Match-V5 ----------

/// `start_time_secs` (epoch-секунды) — серверный фильтр Riot: вернёт только матчи,
/// сыгранные не раньше этого момента. Так старые патчи отсекаются ДО запроса деталей,
/// а не после (раньше каждый старый матч стоил отдельного `fetch_match` и шёл в skip).
pub fn fetch_match_ids(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
    count: usize,
    start_time_secs: Option<i64>,
) -> Result<Vec<String>, RiotError> {
    let cluster = platform_to_match_region(cfg.region.trim());
    let mut url = format!(
        "https://{}.api.riotgames.com/lol/match/v5/matches/by-puuid/{}/ids?queue=420&count={}",
        cluster, puuid, count
    );
    if let Some(start) = start_time_secs {
        url.push_str(&format!("&startTime={}", start));
    }
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    res.json().map_err(|e| RiotError::Network(e.to_string()))
}

/// Последние матчи любой очереди (для истории игрока в скауте).
pub fn fetch_recent_match_ids(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
    count: usize,
) -> Result<Vec<String>, RiotError> {
    let cluster = platform_to_match_region(cfg.region.trim());
    let url = format!(
        "https://{}.api.riotgames.com/lol/match/v5/matches/by-puuid/{}/ids?count={}",
        cluster, puuid, count
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    res.json().map_err(|e| RiotError::Network(e.to_string()))
}

#[derive(Deserialize)]
pub struct MatchParticipant {
    pub puuid: String,
    #[serde(rename = "teamId")]
    pub team_id: i32,
    #[serde(rename = "championId")]
    pub champion_id: i32,
    pub win: bool,
    #[serde(rename = "individualPosition")]
    pub individual_position: String,
    /// Надёжнее individualPosition для агрегации: TOP/JUNGLE/MIDDLE/BOTTOM/UTILITY.
    #[serde(rename = "teamPosition", default)]
    pub team_position: String,
    #[serde(rename = "lane")]
    pub lane: String,
    #[serde(rename = "riotIdGameName", default, deserialize_with = "de_nullable_string")]
    pub riot_id_game_name: String,
    #[serde(rename = "riotIdTagline", default, deserialize_with = "de_nullable_string")]
    pub riot_id_tagline: String,
    #[serde(rename = "summonerName", default, deserialize_with = "de_nullable_string")]
    pub summoner_name: String,
    #[serde(default)]
    pub kills: i32,
    #[serde(default)]
    pub deaths: i32,
    #[serde(default)]
    pub assists: i32,
    #[serde(rename = "totalMinionsKilled", default)]
    pub total_minions_killed: i32,
    #[serde(rename = "neutralMinionsKilled", default)]
    pub neutral_minions_killed: i32,
    pub item0: i32,
    pub item1: i32,
    pub item2: i32,
    pub item3: i32,
    pub item4: i32,
    pub item5: i32,
    pub item6: i32,
    /// Руны игрока (statPerks + styles). Может отсутствовать в очень старых матчах.
    #[serde(default)]
    pub perks: PerksDto,
}

impl MatchParticipant {
    /// Кейстоун — первая выбранная руна основного древа (0 если неизвестно).
    pub fn keystone_id(&self) -> i32 {
        self.perks
            .styles
            .first()
            .and_then(|s| s.selections.first())
            .map(|sel| sel.perk)
            .unwrap_or(0)
    }
    /// ID основного древа рун (0 если неизвестно).
    pub fn primary_style_id(&self) -> i32 {
        self.perks.styles.first().map(|s| s.style).unwrap_or(0)
    }
    /// ID вторичного древа рун (0 если неизвестно).
    pub fn sub_style_id(&self) -> i32 {
        self.perks.styles.get(1).map(|s| s.style).unwrap_or(0)
    }
}

#[derive(Deserialize)]
struct BanDto {
    #[serde(rename = "championId", default)]
    champion_id: i32,
}

#[derive(Deserialize)]
struct TeamDto {
    #[serde(rename = "teamId", default)]
    team_id: i32,
    #[serde(default)]
    bans: Vec<BanDto>,
}

#[derive(Deserialize)]
struct MatchInfo {
    #[serde(rename = "gameVersion")]
    game_version: String,
    #[serde(rename = "queueId")]
    queue_id: i32,
    #[serde(rename = "gameDuration", default)]
    game_duration: i64,
    #[serde(default)]
    teams: Vec<TeamDto>,
    participants: Vec<MatchParticipant>,
}

#[derive(Deserialize)]
struct MatchDto {
    metadata: MatchMetadata,
    info: MatchInfo,
}

#[derive(Deserialize)]
struct MatchMetadata {
    #[serde(rename = "matchId")]
    match_id: String,
}

pub struct ParsedMatch {
    pub match_id: String,
    pub patch: String,
    pub queue_id: i32,
    pub game_duration: i64,
    /// (champion_id, team_id) забаненных чемпионов.
    pub bans: Vec<(i32, i32)>,
    pub participants: Vec<MatchParticipant>,
}

fn extract_patch_bucket(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    if parts.len() >= 2 {
        format!("{}.{}", parts[0], parts[1])
    } else {
        version.to_string()
    }
}

pub fn fetch_match(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    match_id: &str,
) -> Result<ParsedMatch, RiotError> {
    let cluster = platform_to_match_region(cfg.region.trim());
    let url = format!(
        "https://{}.api.riotgames.com/lol/match/v5/matches/{}",
        cluster, match_id
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    let dto: MatchDto = res.json().map_err(|e| RiotError::Network(e.to_string()))?;
    let patch = extract_patch_bucket(&dto.info.game_version);
    let mut bans = Vec::new();
    for t in &dto.info.teams {
        for b in &t.bans {
            if b.champion_id > 0 {
                bans.push((b.champion_id, t.team_id));
            }
        }
    }
    Ok(ParsedMatch {
        match_id: dto.metadata.match_id,
        patch,
        queue_id: dto.info.queue_id,
        game_duration: dto.info.game_duration,
        bans,
        participants: dto.info.participants,
    })
}

// ---------- Match-V5 timeline (порядок покупки предметов) ----------

#[derive(Deserialize)]
struct TimelineEvent {
    #[serde(rename = "type", default)]
    event_type: String,
    #[serde(rename = "participantId", default)]
    participant_id: i32,
    #[serde(rename = "itemId", default)]
    item_id: i32,
}

#[derive(Deserialize)]
struct TimelineFrame {
    #[serde(default)]
    events: Vec<TimelineEvent>,
}

#[derive(Deserialize)]
struct TimelineInfo {
    #[serde(default)]
    frames: Vec<TimelineFrame>,
}

#[derive(Deserialize)]
struct TimelineMetadata {
    #[serde(default)]
    participants: Vec<String>,
}

#[derive(Deserialize)]
struct TimelineDto {
    metadata: TimelineMetadata,
    info: TimelineInfo,
}

/// Покупки предметов в хронологическом порядке: (puuid, item_id).
pub fn fetch_match_timeline(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    match_id: &str,
) -> Result<Vec<(String, i32)>, RiotError> {
    let cluster = platform_to_match_region(cfg.region.trim());
    let url = format!(
        "https://{}.api.riotgames.com/lol/match/v5/matches/{}/timeline",
        cluster, match_id
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    let dto: TimelineDto = res.json().map_err(|e| RiotError::Network(e.to_string()))?;
    let puuids = &dto.metadata.participants;
    let mut purchases = Vec::new();
    for frame in &dto.info.frames {
        for ev in &frame.events {
            if ev.event_type == "ITEM_PURCHASED" && ev.item_id > 0 {
                let idx = (ev.participant_id - 1) as usize;
                if let Some(puuid) = puuids.get(idx) {
                    purchases.push((puuid.clone(), ev.item_id));
                }
            }
        }
    }
    Ok(purchases)
}

pub fn build_enemy_comp_hash(participants: &[MatchParticipant], hero_team_id: i32) -> String {
    let mut ids: Vec<i32> = participants
        .iter()
        .filter(|p| p.team_id != hero_team_id)
        .map(|p| p.champion_id)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join("-")
}

pub fn build_item_hash(p: &MatchParticipant) -> Option<String> {
    let mut items = vec![p.item0, p.item1, p.item2, p.item3, p.item4, p.item5];
    items.retain(|id| *id > 0);
    if items.is_empty() {
        return None;
    }
    items.sort_unstable();
    Some(
        items
            .into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join("-"),
    )
}

// ---------- Spectator-V5 (активная игра) ----------

/// Терпимый к null разбор строки: отсутствует или null → пустая строка.
/// После перехода Riot на Riot ID поле summonerName часто приходит как null.
fn de_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
pub struct ActiveGameParticipant {
    /// PUUID участника — основной идентификатор для определения «своей» команды и рангов.
    #[serde(default, deserialize_with = "de_nullable_string")]
    pub puuid: String,
    /// Riot ID вида "Имя#TAG" для отображения.
    #[serde(rename = "riotId", default, deserialize_with = "de_nullable_string")]
    pub riot_id: String,
    #[serde(rename = "teamId")]
    pub team_id: i32,
    #[serde(rename = "championId")]
    pub champion_id: i32,
    /// Устаревшее поле, может быть пустым/null — только как fallback для отображения.
    #[serde(rename = "summonerName", default, deserialize_with = "de_nullable_string")]
    pub summoner_name: String,
    /// Заклинания лета: Смайт (id 11) → лесник.
    #[serde(rename = "spell1Id", default)]
    pub spell1_id: i32,
    #[serde(rename = "spell2Id", default)]
    pub spell2_id: i32,
}

#[derive(Debug, Deserialize)]
pub struct ActiveGameInfo {
    pub participants: Vec<ActiveGameParticipant>,
}

/// Активная игра по PUUID. ВНИМАНИЕ: в пути написано "by-summoner", но Spectator-V5
/// принимает именно PUUID — числовой summonerId из LCU даёт 400/404.
/// 404 = нет активной игры (это не ошибка).
pub fn fetch_active_game(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
) -> Result<Option<ActiveGameInfo>, RiotError> {
    let url = format!(
        "https://{}.api.riotgames.com/lol/spectator/v5/active-games/by-summoner/{}",
        cfg.region.trim(),
        puuid
    );
    match riot_get(client, limiter, &url, &cfg.api_key) {
        Ok(res) => {
            let info: ActiveGameInfo =
                res.json().map_err(|e| RiotError::Network(e.to_string()))?;
            Ok(Some(info))
        }
        Err(RiotError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

// ---------- Этап 2: сидинг лиг для краулера ----------

/// Сид-игрок из лиги: puuid (если отдан Riot) и/или summonerId для конвертации.
#[derive(Debug, Clone)]
pub struct LeagueSeed {
    pub puuid: String,
    pub summoner_id: String,
}

#[derive(Deserialize)]
struct LeagueItemDto {
    #[serde(default)]
    puuid: String,
    #[serde(rename = "summonerId", default)]
    summoner_id: String,
}

#[derive(Deserialize)]
struct LeagueListDto {
    #[serde(default)]
    entries: Vec<LeagueItemDto>,
}

/// Апекс-лиги одним запросом: queue_path = "challengerleagues" | "grandmasterleagues" | "masterleagues".
pub fn fetch_apex_seeds(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    queue_path: &str,
) -> Result<Vec<LeagueSeed>, RiotError> {
    let url = format!(
        "https://{}.api.riotgames.com/lol/league/v4/{}/by-queue/RANKED_SOLO_5x5",
        cfg.region.trim(),
        queue_path
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    let dto: LeagueListDto = res.json().map_err(|e| RiotError::Network(e.to_string()))?;
    Ok(dto
        .entries
        .into_iter()
        .map(|e| LeagueSeed {
            puuid: e.puuid,
            summoner_id: e.summoner_id,
        })
        .collect())
}

#[derive(Deserialize)]
struct DivisionEntryDto {
    #[serde(default)]
    puuid: String,
    #[serde(rename = "summonerId", default)]
    summoner_id: String,
}

/// Страница дивизиона: tier = "DIAMOND", division = "I".."IV", page >= 1.
/// Пустой результат = страницы кончились.
pub fn fetch_division_seeds(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    tier: &str,
    division: &str,
    page: u32,
) -> Result<Vec<LeagueSeed>, RiotError> {
    let url = format!(
        "https://{}.api.riotgames.com/lol/league/v4/entries/RANKED_SOLO_5x5/{}/{}?page={}",
        cfg.region.trim(),
        tier,
        division,
        page
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    let entries: Vec<DivisionEntryDto> =
        res.json().map_err(|e| RiotError::Network(e.to_string()))?;
    Ok(entries
        .into_iter()
        .map(|e| LeagueSeed {
            puuid: e.puuid,
            summoner_id: e.summoner_id,
        })
        .collect())
}

/// Уровень аккаунта (summoner-v4 by-puuid).
pub fn fetch_summoner_level(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
) -> Result<i64, RiotError> {
    let url = format!(
        "https://{}.api.riotgames.com/lol/summoner/v4/summoners/by-puuid/{}",
        cfg.region.trim(),
        puuid
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    #[derive(serde::Deserialize)]
    struct SummonerDto {
        #[serde(rename = "summonerLevel", default)]
        summoner_level: i64,
    }
    let dto: SummonerDto = res.json().map_err(|e| RiotError::Network(e.to_string()))?;
    Ok(dto.summoner_level)
}

/// Мастерство чемпионов (champion-mastery-v4 by-puuid): (champion_id, level, points),
/// отсортировано по очкам убыв. Платформенный хост (ru/euw1/...).
pub fn fetch_champion_mastery(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    puuid: &str,
) -> Result<Vec<(i32, i32, i64)>, RiotError> {
    #[derive(Deserialize)]
    struct MasteryDto {
        #[serde(rename = "championId", default)]
        champion_id: i32,
        #[serde(rename = "championLevel", default)]
        champion_level: i32,
        #[serde(rename = "championPoints", default)]
        champion_points: i64,
    }
    let url = format!(
        "https://{}.api.riotgames.com/lol/champion-mastery/v4/champion-masteries/by-puuid/{}",
        cfg.region.trim(),
        puuid
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    let list: Vec<MasteryDto> = res.json().map_err(|e| RiotError::Network(e.to_string()))?;
    let mut out: Vec<(i32, i32, i64)> = list
        .into_iter()
        .map(|m| (m.champion_id, m.champion_level, m.champion_points))
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(b.2));
    Ok(out)
}

/// Конвертация summonerId → puuid (если лига не отдала puuid напрямую).
pub fn fetch_puuid_by_summoner_id(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    summoner_id: &str,
) -> Result<String, RiotError> {
    #[derive(Deserialize)]
    struct SummonerDto {
        puuid: String,
    }
    let url = format!(
        "https://{}.api.riotgames.com/lol/summoner/v4/summoners/{}",
        cfg.region.trim(),
        summoner_id
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    let dto: SummonerDto = res.json().map_err(|e| RiotError::Network(e.to_string()))?;
    Ok(dto.puuid)
}

// ---------- Этап 1 разбора матча: полный match-v5 (золото, урон, руны, challenges) ----------

#[derive(Deserialize, Default)]
pub struct PerkSelectionDto {
    #[serde(default)]
    pub perk: i32,
}

#[derive(Deserialize, Default)]
pub struct PerkStyleDto {
    #[serde(default)]
    pub style: i32,
    #[serde(default)]
    pub selections: Vec<PerkSelectionDto>,
}

#[derive(Deserialize, Default)]
pub struct StatPerksDto {
    #[serde(default)]
    pub offense: i32,
    #[serde(default)]
    pub flex: i32,
    #[serde(default)]
    pub defense: i32,
}

#[derive(Deserialize, Default)]
pub struct PerksDto {
    #[serde(rename = "statPerks", default)]
    pub stat_perks: StatPerksDto,
    #[serde(default)]
    pub styles: Vec<PerkStyleDto>,
}

/// Подмножество блока challenges из match-v5 (может отсутствовать в старых матчах).
#[derive(Deserialize, Default)]
struct ChallengesDto {
    #[serde(rename = "soloKills", default)]
    solo_kills: i32,
    #[serde(rename = "killsUnderOwnTurret", default)]
    kills_under_own_turret: i32,
    #[serde(rename = "killParticipation", default)]
    kill_participation: f32,
    #[serde(rename = "teamDamagePercentage", default)]
    team_damage_percentage: f32,
}

#[derive(Deserialize)]
struct ParticipantFullDto {
    #[serde(default, deserialize_with = "de_nullable_string")]
    puuid: String,
    #[serde(rename = "teamId")]
    team_id: i32,
    #[serde(rename = "championId")]
    champion_id: i32,
    win: bool,
    #[serde(rename = "teamPosition", default)]
    team_position: String,
    #[serde(rename = "champLevel", default)]
    champ_level: i32,
    #[serde(default)]
    kills: i32,
    #[serde(default)]
    deaths: i32,
    #[serde(default)]
    assists: i32,
    #[serde(rename = "totalMinionsKilled", default)]
    total_minions_killed: i32,
    #[serde(rename = "neutralMinionsKilled", default)]
    neutral_minions_killed: i32,
    #[serde(rename = "goldEarned", default)]
    gold_earned: i32,
    #[serde(rename = "totalDamageDealtToChampions", default)]
    damage_to_champions: i32,
    #[serde(rename = "visionScore", default)]
    vision_score: i32,
    #[serde(rename = "wardsPlaced", default)]
    wards_placed: i32,
    #[serde(rename = "wardsKilled", default)]
    wards_killed: i32,
    #[serde(rename = "visionWardsBoughtInGame", default)]
    control_wards: i32,
    #[serde(rename = "totalDamageTaken", default)]
    damage_taken: i32,
    #[serde(default)]
    item0: i32,
    #[serde(default)]
    item1: i32,
    #[serde(default)]
    item2: i32,
    #[serde(default)]
    item3: i32,
    #[serde(default)]
    item4: i32,
    #[serde(default)]
    item5: i32,
    #[serde(default)]
    item6: i32,
    #[serde(rename = "riotIdGameName", default, deserialize_with = "de_nullable_string")]
    riot_id_game_name: String,
    #[serde(rename = "riotIdTagline", default, deserialize_with = "de_nullable_string")]
    riot_id_tagline: String,
    #[serde(rename = "summonerName", default, deserialize_with = "de_nullable_string")]
    summoner_name: String,
    #[serde(default)]
    perks: PerksDto,
    #[serde(default)]
    challenges: ChallengesDto,
}

/// Один участник в полном разборе матча.
pub struct ParticipantFull {
    pub puuid: String,
    pub riot_id: String,
    pub champion_id: i32,
    pub team_id: i32,
    pub role: String,
    pub champ_level: i32,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub cs: i32,
    pub gold: i32,
    pub damage_to_champions: i32,
    pub damage_taken: i32,
    pub vision_score: i32,
    pub wards_placed: i32,
    pub wards_killed: i32,
    pub control_wards: i32,
    pub items: Vec<u32>,
    pub keystone_id: i32,
    pub primary_style_id: i32,
    pub sub_style_id: i32,
    /// Полные руны: основное древо (4 руны), вторичное (2), осколки статов (3).
    pub primary_perks: Vec<i32>,
    pub sub_perks: Vec<i32>,
    pub stat_perks: Vec<i32>,
    pub solo_kills: i32,
    pub kills_under_turret: i32,
    pub kill_participation: f32,
    pub team_damage_percentage: f32,
    pub win: bool,
}

/// Объективы команды (счётчики взятых целей).
pub struct TeamObjectives {
    pub team_id: i32,
    pub baron: i32,
    pub dragon: i32,
    pub herald: i32,
    pub tower: i32,
    pub inhibitor: i32,
}

pub struct MatchFull {
    pub match_id: String,
    pub queue_id: i32,
    pub patch: String,
    pub game_duration: i64,
    pub participants: Vec<ParticipantFull>,
    pub teams: Vec<TeamObjectives>,
}

#[derive(Deserialize, Default)]
struct ObjCountDto {
    #[serde(default)]
    kills: i32,
}

#[derive(Deserialize, Default)]
struct ObjectivesDto {
    #[serde(default)]
    baron: ObjCountDto,
    #[serde(default)]
    dragon: ObjCountDto,
    #[serde(rename = "riftHerald", default)]
    rift_herald: ObjCountDto,
    #[serde(default)]
    tower: ObjCountDto,
    #[serde(default)]
    inhibitor: ObjCountDto,
}

#[derive(Deserialize)]
struct TeamObjDto {
    #[serde(rename = "teamId", default)]
    team_id: i32,
    #[serde(default)]
    objectives: ObjectivesDto,
}

#[derive(Deserialize)]
struct MatchInfoFull {
    #[serde(rename = "gameVersion")]
    game_version: String,
    #[serde(rename = "queueId")]
    queue_id: i32,
    #[serde(rename = "gameDuration", default)]
    game_duration: i64,
    #[serde(default)]
    teams: Vec<TeamObjDto>,
    participants: Vec<ParticipantFullDto>,
}

#[derive(Deserialize)]
struct MatchFullDto {
    metadata: MatchMetadata,
    info: MatchInfoFull,
}

fn full_display_name(p: &ParticipantFullDto) -> String {
    if !p.riot_id_game_name.is_empty() {
        if p.riot_id_tagline.is_empty() {
            p.riot_id_game_name.clone()
        } else {
            format!("{}#{}", p.riot_id_game_name, p.riot_id_tagline)
        }
    } else if !p.summoner_name.is_empty() {
        p.summoner_name.clone()
    } else {
        "—".to_string()
    }
}

/// Полный разбор одного матча: золото, урон, руны, challenges каждого игрока.
pub fn fetch_match_full(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    match_id: &str,
) -> Result<MatchFull, RiotError> {
    let cluster = platform_to_match_region(cfg.region.trim());
    let url = format!(
        "https://{}.api.riotgames.com/lol/match/v5/matches/{}",
        cluster, match_id
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    let dto: MatchFullDto = res.json().map_err(|e| RiotError::Network(e.to_string()))?;
    let patch = extract_patch_bucket(&dto.info.game_version);

    let participants = dto
        .info
        .participants
        .into_iter()
        .map(|p| {
            let primary_perks: Vec<i32> = p
                .perks
                .styles
                .first()
                .map(|s| s.selections.iter().map(|x| x.perk).collect())
                .unwrap_or_default();
            let sub_perks: Vec<i32> = p
                .perks
                .styles
                .get(1)
                .map(|s| s.selections.iter().map(|x| x.perk).collect())
                .unwrap_or_default();
            let stat_perks = vec![
                p.perks.stat_perks.offense,
                p.perks.stat_perks.flex,
                p.perks.stat_perks.defense,
            ];
            let keystone_id = primary_perks.first().copied().unwrap_or(0);
            let primary_style_id = p.perks.styles.first().map(|s| s.style).unwrap_or(0);
            let sub_style_id = p.perks.styles.get(1).map(|s| s.style).unwrap_or(0);
            let items = [p.item0, p.item1, p.item2, p.item3, p.item4, p.item5, p.item6]
                .into_iter()
                .filter(|id| *id > 0)
                .map(|id| id as u32)
                .collect();
            ParticipantFull {
                riot_id: full_display_name(&p),
                puuid: p.puuid,
                champion_id: p.champion_id,
                team_id: p.team_id,
                role: p.team_position.clone(),
                champ_level: p.champ_level,
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
                cs: p.total_minions_killed + p.neutral_minions_killed,
                gold: p.gold_earned,
                damage_to_champions: p.damage_to_champions,
                damage_taken: p.damage_taken,
                vision_score: p.vision_score,
                wards_placed: p.wards_placed,
                wards_killed: p.wards_killed,
                control_wards: p.control_wards,
                items,
                keystone_id,
                primary_style_id,
                sub_style_id,
                primary_perks,
                sub_perks,
                stat_perks,
                solo_kills: p.challenges.solo_kills,
                kills_under_turret: p.challenges.kills_under_own_turret,
                kill_participation: p.challenges.kill_participation,
                team_damage_percentage: p.challenges.team_damage_percentage,
                win: p.win,
            }
        })
        .collect();

    let teams = dto
        .info
        .teams
        .iter()
        .map(|t| TeamObjectives {
            team_id: t.team_id,
            baron: t.objectives.baron.kills,
            dragon: t.objectives.dragon.kills,
            herald: t.objectives.rift_herald.kills,
            tower: t.objectives.tower.kills,
            inhibitor: t.objectives.inhibitor.kills,
        })
        .collect();

    Ok(MatchFull {
        match_id: dto.metadata.match_id,
        queue_id: dto.info.queue_id,
        patch,
        game_duration: dto.info.game_duration,
        participants,
        teams,
    })
}

// ---------- Этап 2/3 разбора: таймлайн (метрики по минутам + покупки + события) ----------

#[derive(Deserialize, Default)]
struct ParticipantFrameDto {
    #[serde(rename = "totalGold", default)]
    total_gold: i32,
    #[serde(default)]
    xp: i32,
    #[serde(default)]
    level: i32,
    #[serde(rename = "minionsKilled", default)]
    minions_killed: i32,
    #[serde(rename = "jungleMinionsKilled", default)]
    jungle_minions_killed: i32,
    /// Позиция чемпиона на момент кадра (x,y, карта ~0..15000). Riot отдаёт это
    /// поле в `participantFrames`, но раньше оно не разбиралось — нужно для
    /// тепловой карты позиций игрока по минутам (player-pattern engine).
    #[serde(default)]
    position: Option<PositionDto>,
}

#[derive(Deserialize, Default)]
struct PositionDto {
    #[serde(default)]
    x: i32,
    #[serde(default)]
    y: i32,
}

#[derive(Deserialize)]
struct TlEventDto {
    #[serde(rename = "type", default)]
    event_type: String,
    #[serde(default)]
    timestamp: i64,
    #[serde(rename = "participantId", default)]
    participant_id: i32,
    #[serde(rename = "itemId", default)]
    item_id: i32,
    #[serde(rename = "beforeId", default)]
    before_id: i32,
    #[serde(rename = "killerId", default)]
    killer_id: i32,
    #[serde(rename = "victimId", default)]
    victim_id: i32,
    #[serde(default)]
    position: Option<PositionDto>,
    #[serde(rename = "monsterType", default)]
    monster_type: String,
    #[serde(rename = "monsterSubType", default)]
    monster_sub_type: String,
    #[serde(rename = "buildingType", default)]
    building_type: String,
    #[serde(rename = "towerType", default)]
    tower_type: String,
    #[serde(rename = "teamId", default)]
    team_id: i32,
    #[serde(rename = "killerTeamId", default)]
    killer_team_id: i32,
}

#[derive(Deserialize)]
struct TlFrameDto {
    #[serde(default)]
    timestamp: i64,
    #[serde(rename = "participantFrames", default)]
    participant_frames: std::collections::HashMap<String, ParticipantFrameDto>,
    #[serde(default)]
    events: Vec<TlEventDto>,
}

#[derive(Deserialize)]
struct TlInfoDto {
    #[serde(default)]
    frames: Vec<TlFrameDto>,
}

#[derive(Deserialize)]
struct TlMetaDto {
    #[serde(default)]
    participants: Vec<String>,
}

#[derive(Deserialize)]
struct TlDtoFull {
    metadata: TlMetaDto,
    info: TlInfoDto,
}

/// Метрики всех участников на момент кадра (индекс = participantId-1).
pub struct FrameStats {
    pub timestamp_ms: i64,
    pub gold: Vec<i32>,
    pub xp: Vec<i32>,
    pub level: Vec<i32>,
    pub cs: Vec<i32>,
    /// Позиция каждого участника на момент кадра: (x, y), карта ~0..15000.
    /// (0,0) — позиция неизвестна (поле отсутствовало в кадре).
    pub positions: Vec<(i32, i32)>,
}

/// Покупка предмета (undo уже учтены и удалены).
pub struct PurchaseEvent {
    pub participant_id: i32,
    pub item_id: i32,
    pub timestamp_ms: i64,
}

/// Игровое событие: убийство / элитный монстр / здание.
pub struct GameEvent {
    pub timestamp_ms: i64,
    /// "kill" | "monster" | "building".
    pub kind: String,
    pub killer_id: i32,
    pub victim_id: i32,
    /// Для монстра — команда-убийца; для здания — команда, потерявшая строение.
    pub team_id: i32,
    /// Тип монстра/здания (напр. "AIR_DRAGON", "BARON_NASHOR", "TOWER_BUILDING").
    pub detail: String,
    /// Координаты события на карте (для убийств — место смерти). Карта ~0..15000.
    pub x: i32,
    pub y: i32,
}

pub struct TimelineFull {
    /// puuid по индексу participantId-1.
    pub participant_puuids: Vec<String>,
    pub frames: Vec<FrameStats>,
    pub purchases: Vec<PurchaseEvent>,
    pub events: Vec<GameEvent>,
}

/// Полный таймлайн матча: метрики по кадрам, покупки и события.
pub fn fetch_match_timeline_full(
    client: &Client,
    cfg: &RiotConfig,
    limiter: &SharedLimiter,
    match_id: &str,
) -> Result<TimelineFull, RiotError> {
    let cluster = platform_to_match_region(cfg.region.trim());
    let url = format!(
        "https://{}.api.riotgames.com/lol/match/v5/matches/{}/timeline",
        cluster, match_id
    );
    let res = riot_get(client, limiter, &url, &cfg.api_key)?;
    let dto: TlDtoFull = res.json().map_err(|e| RiotError::Network(e.to_string()))?;

    let mut frames = Vec::with_capacity(dto.info.frames.len());
    let mut purchases: Vec<PurchaseEvent> = Vec::new();
    let mut events: Vec<GameEvent> = Vec::new();

    for frame in &dto.info.frames {
        // Метрики по участникам (1..10) на момент кадра.
        let mut gold = vec![0i32; 10];
        let mut xp = vec![0i32; 10];
        let mut level = vec![0i32; 10];
        let mut cs = vec![0i32; 10];
        let mut positions = vec![(0i32, 0i32); 10];
        for (pid, pf) in &frame.participant_frames {
            if let Ok(idx) = pid.parse::<usize>() {
                if (1..=10).contains(&idx) {
                    let i = idx - 1;
                    gold[i] = pf.total_gold;
                    xp[i] = pf.xp;
                    level[i] = pf.level;
                    cs[i] = pf.minions_killed + pf.jungle_minions_killed;
                    if let Some(pos) = &pf.position {
                        positions[i] = (pos.x, pos.y);
                    }
                }
            }
        }
        frames.push(FrameStats {
            timestamp_ms: frame.timestamp,
            gold,
            xp,
            level,
            cs,
            positions,
        });

        // Покупки/отмены и игровые события в хронологическом порядке.
        for ev in &frame.events {
            match ev.event_type.as_str() {
                "ITEM_PURCHASED" if ev.item_id > 0 => purchases.push(PurchaseEvent {
                    participant_id: ev.participant_id,
                    item_id: ev.item_id,
                    timestamp_ms: ev.timestamp,
                }),
                "ITEM_UNDO" => {
                    if let Some(pos) = purchases.iter().rposition(|p| {
                        p.participant_id == ev.participant_id && p.item_id == ev.before_id
                    }) {
                        purchases.remove(pos);
                    }
                }
                "CHAMPION_KILL" => {
                    let (x, y) = ev.position.as_ref().map(|p| (p.x, p.y)).unwrap_or((0, 0));
                    events.push(GameEvent {
                        timestamp_ms: ev.timestamp,
                        kind: "kill".into(),
                        killer_id: ev.killer_id,
                        victim_id: ev.victim_id,
                        team_id: 0,
                        detail: String::new(),
                        x,
                        y,
                    });
                }
                "ELITE_MONSTER_KILL" => {
                    let detail = if !ev.monster_sub_type.is_empty() {
                        ev.monster_sub_type.clone()
                    } else {
                        ev.monster_type.clone()
                    };
                    let (x, y) = ev.position.as_ref().map(|p| (p.x, p.y)).unwrap_or((0, 0));
                    events.push(GameEvent {
                        timestamp_ms: ev.timestamp,
                        kind: "monster".into(),
                        killer_id: ev.killer_id,
                        victim_id: 0,
                        team_id: ev.killer_team_id,
                        detail,
                        x,
                        y,
                    });
                }
                "BUILDING_KILL" => {
                    let detail = if !ev.tower_type.is_empty() {
                        ev.tower_type.clone()
                    } else {
                        ev.building_type.clone()
                    };
                    let (x, y) = ev.position.as_ref().map(|p| (p.x, p.y)).unwrap_or((0, 0));
                    events.push(GameEvent {
                        timestamp_ms: ev.timestamp,
                        kind: "building".into(),
                        killer_id: ev.killer_id,
                        victim_id: 0,
                        team_id: ev.team_id,
                        detail,
                        x,
                        y,
                    });
                }
                _ => {}
            }
        }
    }

    Ok(TimelineFull {
        participant_puuids: dto.metadata.participants,
        frames,
        purchases,
        events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_mappers() {
        assert_eq!(platform_to_account_region("ru"), "europe");
        assert_eq!(platform_to_account_region("oc1"), "asia");
        assert_eq!(platform_to_match_region("ru"), "europe");
        assert_eq!(platform_to_match_region("oc1"), "sea");
        assert_eq!(platform_to_match_region("kr"), "asia");
    }

    #[test]
    fn parse_active_game_participant() {
        let json = r#"{
            "puuid": "abc-123",
            "riotId": "Игрок#RU1",
            "teamId": 100,
            "championId": 54,
            "summonerName": ""
        }"#;
        let p: ActiveGameParticipant = serde_json::from_str(json).unwrap();
        assert_eq!(p.puuid, "abc-123");
        assert_eq!(p.riot_id, "Игрок#RU1");
        assert_eq!(p.champion_id, 54);
    }

    #[test]
    fn parse_account() {
        let json = r#"{"puuid":"xyz","gameName":"Имя","tagLine":"RU1"}"#;
        let a: RiotAccount = serde_json::from_str(json).unwrap();
        assert_eq!(a.puuid, "xyz");
        assert_eq!(a.tag_line, "RU1");
    }

    #[test]
    fn emerald_check() {
        assert!(is_emerald_or_higher("EMERALD"));
        assert!(is_emerald_or_higher("Challenger"));
        assert!(!is_emerald_or_higher("PLATINUM"));
    }

    #[test]
    fn patch_bucket() {
        assert_eq!(extract_patch_bucket("16.5.123.456"), "16.5");
        assert_eq!(extract_patch_bucket("16"), "16");
    }
}
