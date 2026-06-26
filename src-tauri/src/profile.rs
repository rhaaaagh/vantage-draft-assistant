use crate::ddragon::display_name;
use crate::riot_api::{
    fetch_account_by_riot_id, fetch_league_rank, fetch_match, fetch_recent_match_ids,
    fetch_summoner_level, RiotConfig, RiotError, SharedLimiter,
};
use reqwest::blocking::Client;
use std::collections::HashMap;

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePerChampion {
    pub champion_id: u32,
    pub champion_name: String,
    pub games: u32,
    pub wins: u32,
}

/// Один участник матча (для развёрнутой карточки игры).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPlayer {
    pub puuid: String,
    pub riot_id: String,
    pub champion_id: u32,
    pub champion_name: String,
    pub team_id: i32,
    pub role: String,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub cs: i32,
    pub win: bool,
    pub items: Vec<u32>,
    /// true для строки самого искомого игрока (подсветить).
    pub is_target: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileGame {
    pub match_id: String,
    pub champion_id: u32,
    pub champion_name: String,
    pub win: bool,
    pub queue_id: i32,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub cs: i32,
    /// Длительность игры в секундах.
    pub game_duration: i64,
    /// Предметы (item0..item6), нули отфильтрованы.
    pub items: Vec<u32>,
    /// Все 10 участников матча (для развёрнутой карточки).
    pub participants: Vec<MatchPlayer>,
}

fn player_items(p: &crate::riot_api::MatchParticipant) -> Vec<u32> {
    [p.item0, p.item1, p.item2, p.item3, p.item4, p.item5, p.item6]
        .into_iter()
        .filter(|id| *id > 0)
        .map(|id| id as u32)
        .collect()
}

fn player_display_name(p: &crate::riot_api::MatchParticipant) -> String {
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileResponse {
    pub found: bool,
    pub error_message: Option<String>,
    pub puuid: String,
    pub riot_id: String,
    pub summoner_level: i64,
    pub tier: String,
    pub rank: String,
    pub league_points: i32,
    pub wins: i32,
    pub losses: i32,
    pub per_champion: Vec<ProfilePerChampion>,
    pub history: Vec<ProfileGame>,
}

fn empty(found: bool, err: Option<String>) -> ProfileResponse {
    ProfileResponse {
        found,
        error_message: err,
        puuid: String::new(),
        riot_id: String::new(),
        summoner_level: 0,
        tier: String::new(),
        rank: String::new(),
        league_points: 0,
        wins: 0,
        losses: 0,
        per_champion: vec![],
        history: vec![],
    }
}

pub fn fetch_profile_impl(
    limiter: &SharedLimiter,
    api_key: String,
    region: String,
    game_name: String,
    tag_line: String,
    history_count: u32,
) -> Result<ProfileResponse, String> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("Введите Riot API ключ в Настройках.".into());
    }
    if game_name.trim().is_empty() || tag_line.trim().is_empty() {
        return Err("Введите Riot ID в формате Имя#TAG.".into());
    }
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let cfg = RiotConfig {
        api_key,
        region: region.trim().to_string(),
    };

    let account = match fetch_account_by_riot_id(&client, &cfg, limiter, &game_name, &tag_line) {
        Ok(a) => a,
        Err(RiotError::NotFound) => {
            return Ok(empty(
                false,
                Some(format!(
                    "Riot ID {}#{} не найден.",
                    game_name.trim(),
                    tag_line.trim()
                )),
            ))
        }
        Err(e) => return Err(e.to_user_message()),
    };

    let level = fetch_summoner_level(&client, &cfg, limiter, &account.puuid).unwrap_or(0);
    let rank = fetch_league_rank(&client, &cfg, limiter, &account.puuid).unwrap_or(None);
    let (tier, rank_div, lp, wins, losses) = match rank {
        Some(r) => (r.tier, r.rank, r.league_points, r.wins, r.losses),
        None => (String::new(), String::new(), 0, 0, 0),
    };

    let ids = fetch_recent_match_ids(
        &client,
        &cfg,
        limiter,
        &account.puuid,
        history_count.clamp(1, 30) as usize,
    )
    .map_err(|e| e.to_user_message())?;

    let mut history = Vec::new();
    let mut agg: HashMap<i32, (u32, u32)> = HashMap::new(); // champ -> (games, wins)
    for id in ids {
        let parsed = match fetch_match(&client, &cfg, limiter, &id) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if let Some(me) = parsed.participants.iter().find(|p| p.puuid == account.puuid) {
            let e = agg.entry(me.champion_id).or_insert((0, 0));
            e.0 += 1;
            if me.win {
                e.1 += 1;
            }
            let me_champ = me.champion_id;
            let me_kills = me.kills;
            let me_deaths = me.deaths;
            let me_assists = me.assists;
            let me_cs = me.total_minions_killed + me.neutral_minions_killed;
            let me_win = me.win;
            let me_items = player_items(me);

            // Все 10 участников для развёрнутой карточки.
            let participants: Vec<MatchPlayer> = parsed
                .participants
                .iter()
                .map(|p| MatchPlayer {
                    puuid: p.puuid.clone(),
                    riot_id: player_display_name(p),
                    champion_id: p.champion_id as u32,
                    champion_name: display_name(p.champion_id as u32),
                    team_id: p.team_id,
                    role: p.team_position.clone(),
                    kills: p.kills,
                    deaths: p.deaths,
                    assists: p.assists,
                    cs: p.total_minions_killed + p.neutral_minions_killed,
                    win: p.win,
                    items: player_items(p),
                    is_target: p.puuid == account.puuid,
                })
                .collect();

            history.push(ProfileGame {
                match_id: parsed.match_id.clone(),
                champion_id: me_champ as u32,
                champion_name: display_name(me_champ as u32),
                win: me_win,
                queue_id: parsed.queue_id,
                kills: me_kills,
                deaths: me_deaths,
                assists: me_assists,
                cs: me_cs,
                game_duration: parsed.game_duration,
                items: me_items,
                participants,
            });
        }
    }
    let mut per_champion: Vec<ProfilePerChampion> = agg
        .into_iter()
        .map(|(cid, (g, w))| ProfilePerChampion {
            champion_id: cid as u32,
            champion_name: display_name(cid as u32),
            games: g,
            wins: w,
        })
        .collect();
    per_champion.sort_by_key(|b| std::cmp::Reverse(b.games));

    Ok(ProfileResponse {
        found: true,
        error_message: None,
        puuid: account.puuid.clone(),
        riot_id: format!("{}#{}", account.game_name, account.tag_line),
        summoner_level: level,
        tier,
        rank: rank_div,
        league_points: lp,
        wins,
        losses,
        per_champion,
        history,
    })
}
