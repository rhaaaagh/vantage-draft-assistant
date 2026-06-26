use crate::ddragon::display_name;
use crate::riot_api::{
    fetch_match_full, fetch_match_timeline_full, RiotConfig, SharedLimiter,
};
use reqwest::blocking::Client;

/// Одна покупка предмета в порядке игры.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PurchaseItem {
    pub item_id: u32,
    /// Минута игры, на которой куплен.
    pub minute: i32,
}

/// Срез метрик на минуте (массивы по индексу = порядок players).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameSnapshot {
    pub minute: i32,
    pub gold: Vec<i32>,
    pub xp: Vec<i32>,
    pub level: Vec<i32>,
    pub cs: Vec<i32>,
}

/// Событие матча для ленты (killerId/victimId — participantId 1..10).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchEvent {
    pub minute: i32,
    pub second: i32,
    /// "kill" | "monster" | "building".
    pub kind: String,
    pub killer_id: i32,
    pub victim_id: i32,
    pub team_id: i32,
    pub detail: String,
    pub x: i32,
    pub y: i32,
}

/// Один игрок в полноэкранном разборе матча.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDetailPlayer {
    pub puuid: String,
    pub riot_id: String,
    pub champion_id: u32,
    pub champion_name: String,
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
    /// Ключевая руна (для иконки в строке).
    pub keystone_id: i32,
    /// Основное и дополнительное древо рун (id стиля).
    pub primary_style_id: i32,
    pub sub_style_id: i32,
    /// Полные руны для раскрытой карточки: основное древо (4), вторичное (2), осколки (3).
    pub primary_perks: Vec<i32>,
    pub sub_perks: Vec<i32>,
    pub stat_perks: Vec<i32>,
    pub solo_kills: i32,
    pub kills_under_turret: i32,
    /// 0..1.
    pub kill_participation: f32,
    /// 0..1.
    pub team_damage_percentage: f32,
    /// Подсветить строку искомого игрока.
    pub is_target: bool,
    /// Последовательность покупок (Этап 2, из таймлайна; пусто если таймлайн недоступен).
    pub purchases: Vec<PurchaseItem>,
}

/// Итог по команде (для шапки и сравнения).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchTeamSummary {
    pub team_id: i32,
    pub win: bool,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub gold: i32,
    pub baron: i32,
    pub dragon: i32,
    pub herald: i32,
    pub tower: i32,
    pub inhibitor: i32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchDetailResponse {
    pub match_id: String,
    pub queue_id: i32,
    pub patch: String,
    pub game_duration: i64,
    pub teams: Vec<MatchTeamSummary>,
    pub players: Vec<MatchDetailPlayer>,
    /// Преимущество команды искомого игрока по золоту по минутам (может быть < 0).
    /// Пусто, если таймлайн недоступен.
    pub gold_advantage: Vec<i32>,
    /// Метрики по минутам (для графиков и ползунка). Пусто без таймлайна.
    pub frames: Vec<FrameSnapshot>,
    /// Лента событий матча. Пусто без таймлайна.
    pub events: Vec<MatchEvent>,
}

pub fn fetch_match_detail_impl(
    limiter: &SharedLimiter,
    api_key: String,
    region: String,
    match_id: String,
    target_puuid: String,
) -> Result<MatchDetailResponse, String> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("Введите Riot API ключ в Настройках.".into());
    }
    if match_id.trim().is_empty() {
        return Err("Не указан матч.".into());
    }
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let cfg = RiotConfig {
        api_key,
        region: region.trim().to_string(),
    };

    let full = fetch_match_full(&client, &cfg, limiter, match_id.trim())
        .map_err(|e| e.to_user_message())?;

    let mut players: Vec<MatchDetailPlayer> = full
        .participants
        .iter()
        .map(|p| MatchDetailPlayer {
            purchases: Vec::new(),
            is_target: !target_puuid.is_empty() && p.puuid == target_puuid,
            puuid: p.puuid.clone(),
            riot_id: p.riot_id.clone(),
            champion_id: p.champion_id as u32,
            champion_name: display_name(p.champion_id as u32),
            team_id: p.team_id,
            role: p.role.clone(),
            champ_level: p.champ_level,
            kills: p.kills,
            deaths: p.deaths,
            assists: p.assists,
            cs: p.cs,
            gold: p.gold,
            damage_to_champions: p.damage_to_champions,
            damage_taken: p.damage_taken,
            vision_score: p.vision_score,
            wards_placed: p.wards_placed,
            wards_killed: p.wards_killed,
            control_wards: p.control_wards,
            items: p.items.clone(),
            keystone_id: p.keystone_id,
            primary_style_id: p.primary_style_id,
            sub_style_id: p.sub_style_id,
            primary_perks: p.primary_perks.clone(),
            sub_perks: p.sub_perks.clone(),
            stat_perks: p.stat_perks.clone(),
            solo_kills: p.solo_kills,
            kills_under_turret: p.kills_under_turret,
            kill_participation: p.kill_participation,
            team_damage_percentage: p.team_damage_percentage,
        })
        .collect();

    // Итоги по командам (100/200): сумма киллов/смертей/ассистов/золота + исход.
    let mut teams: Vec<MatchTeamSummary> = Vec::new();
    for team_id in [100, 200] {
        let members: Vec<&crate::riot_api::ParticipantFull> =
            full.participants.iter().filter(|p| p.team_id == team_id).collect();
        if members.is_empty() {
            continue;
        }
        let obj = full.teams.iter().find(|t| t.team_id == team_id);
        teams.push(MatchTeamSummary {
            team_id,
            win: members.first().map(|p| p.win).unwrap_or(false),
            kills: members.iter().map(|p| p.kills).sum(),
            deaths: members.iter().map(|p| p.deaths).sum(),
            assists: members.iter().map(|p| p.assists).sum(),
            gold: members.iter().map(|p| p.gold).sum(),
            baron: obj.map(|o| o.baron).unwrap_or(0),
            dragon: obj.map(|o| o.dragon).unwrap_or(0),
            herald: obj.map(|o| o.herald).unwrap_or(0),
            tower: obj.map(|o| o.tower).unwrap_or(0),
            inhibitor: obj.map(|o| o.inhibitor).unwrap_or(0),
        });
    }

    // Команда искомого игрока (для ориентации графика золота). По умолчанию — 100.
    let target_team = players
        .iter()
        .find(|p| p.is_target)
        .map(|p| p.team_id)
        .unwrap_or(100);

    // Таймлайн — best-effort: при ошибке просто без графиков/покупок/событий.
    let mut gold_advantage: Vec<i32> = Vec::new();
    let mut frames: Vec<FrameSnapshot> = Vec::new();
    let mut events: Vec<MatchEvent> = Vec::new();
    if let Ok(tl) = fetch_match_timeline_full(&client, &cfg, limiter, match_id.trim()) {
        // График: преимущество команды искомого игрока по золоту по кадрам.
        gold_advantage = tl
            .frames
            .iter()
            .map(|f| {
                let team100: i32 = f.gold.iter().take(5).sum();
                let team200: i32 = f.gold.iter().skip(5).take(5).sum();
                if target_team == 200 {
                    team200 - team100
                } else {
                    team100 - team200
                }
            })
            .collect();

        // Срезы метрик по минутам (для графиков и ползунка).
        frames = tl
            .frames
            .iter()
            .map(|f| FrameSnapshot {
                minute: (f.timestamp_ms / 60_000) as i32,
                gold: f.gold.clone(),
                xp: f.xp.clone(),
                level: f.level.clone(),
                cs: f.cs.clone(),
            })
            .collect();

        // Лента событий.
        events = tl
            .events
            .iter()
            .map(|e| MatchEvent {
                minute: (e.timestamp_ms / 60_000) as i32,
                second: ((e.timestamp_ms / 1000) % 60) as i32,
                kind: e.kind.clone(),
                killer_id: e.killer_id,
                victim_id: e.victim_id,
                team_id: e.team_id,
                detail: e.detail.clone(),
                x: e.x,
                y: e.y,
            })
            .collect();

        // Покупки → игрокам (participantId → puuid → игрок).
        for ev in &tl.purchases {
            let idx = (ev.participant_id - 1) as usize;
            if let Some(puuid) = tl.participant_puuids.get(idx) {
                if let Some(player) = players.iter_mut().find(|p| &p.puuid == puuid) {
                    player.purchases.push(PurchaseItem {
                        item_id: ev.item_id as u32,
                        minute: (ev.timestamp_ms / 60_000) as i32,
                    });
                }
            }
        }
    }

    Ok(MatchDetailResponse {
        match_id: full.match_id,
        queue_id: full.queue_id,
        patch: full.patch,
        game_duration: full.game_duration,
        teams,
        players,
        gold_advantage,
        frames,
        events,
    })
}
