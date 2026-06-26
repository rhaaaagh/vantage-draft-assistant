#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// Tauri-команды объективно принимают много параметров (apiKey, region, puuid,
// фильтры…) и иногда возвращают составные типы — это форма IPC, а не запах кода.
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]

mod lcu;
mod db;
mod ddragon;
mod crawler;
mod paths;
mod rate_limit;
mod riot_api;
mod profile;
mod match_detail;
mod recommend;
mod archetypes;

use serde::Serialize;
use std::io::Write;
use std::panic;

fn log_path() -> std::path::PathBuf {
    paths::log_path()
}

fn log_msg(msg: &str) {
    let path = log_path();
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {}", chrono_lite(), msg);
        let _ = f.flush();
    }
}

/// Пишет в лог и на stderr (для отладки в консоли).
fn log_crash(msg: &str) {
    let path = log_path();
    let line = format!("{} [CRASH] {}", chrono_lite(), msg);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{}", line);
        let _ = f.flush();
    }
    eprintln!("{}", line);
}

fn chrono_lite() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("[{}.{:03}]", t.as_secs(), t.subsec_millis())
}
use lcu::{champion_display_name, fetch_champ_select_state, check_lcu_and_fetch_session, get_current_platform, get_current_summoner};
use db::{Database, ChampionVsCompAgg, BuildAgg};
use riot_api::{
    RiotConfig,
    SharedLimiter,
    fetch_account_by_riot_id,
    fetch_is_emerald_plus,
    fetch_match_ids,
    fetch_match,
    build_enemy_comp_hash,
    build_item_hash,
    fetch_active_game,
    fetch_league_entry_display,
};
use reqwest::blocking::Client;
use std::path::PathBuf;
use std::sync::Arc;

/// Общее состояние приложения (управляется Tauri).
struct AppState {
    limiter: SharedLimiter,
    catalog: ddragon::SharedCatalog,
    crawl: crawler::CrawlControl,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TeamSide {
    Blue,
    Red,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Role {
    Top,
    Jungle,
    Mid,
    Bot,
    Support,
}

impl Role {
    /// Строковый код роли для движка рекомендаций: TOP/JUNGLE/MID/BOT/SUPPORT.
    fn as_engine_str(&self) -> &'static str {
        match self {
            Role::Top => "TOP",
            Role::Jungle => "JUNGLE",
            Role::Mid => "MID",
            Role::Bot => "BOT",
            Role::Support => "SUPPORT",
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DraftSlot {
    pub champion_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub champion_name: Option<String>,
    pub role: Option<Role>,
    pub player_name: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct TeamDraft {
    pub side: TeamSide,
    pub slots: Vec<DraftSlot>,
    pub bans: Vec<u32>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Phase {
    None,
    ChampSelect,
}

#[derive(Serialize, Clone)]
pub struct DraftState {
    pub phase: Phase,
    pub blue: TeamDraft,
    pub red: TeamDraft,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PickSuggestion {
    champion_id: u32,
    champion_name: String,
    winrate: f32,
    games: u32,
    reason: String,
}

#[derive(Serialize, Clone)]
struct BuildItem {
    id: u32,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildRecommendation {
    champion_id: u32,
    champion_name: String,
    best_winrate_build: Vec<BuildItem>,
    most_popular_build: Vec<BuildItem>,
    vs_enemy_build: Vec<BuildItem>,
}

#[derive(Serialize)]
struct DamageProfile {
    ad: f32,
    ap: f32,
}

/// Аналитика драфта. Метрики без реального источника данных — None
/// (фронт скрывает их вместо показа выдуманных цифр).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftAnalytics {
    /// Появится в Этапе 2 на реальных данных матчапов.
    blue_win_probability: Option<f32>,
    red_win_probability: Option<f32>,
    /// Появится в Этапе 2.
    blue_synergy_score: Option<f32>,
    red_synergy_score: Option<f32>,
    /// Оценка по Data Dragon (attack/magic рейтинги чемпионов).
    blue_damage_profile: DamageProfile,
    red_damage_profile: DamageProfile,
    blue_weaknesses: Vec<String>,
    red_weaknesses: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftAnalysisResult {
    draft: DraftState,
    /// Баны на верхнем уровне — фронт читает их отсюда, чтобы не терять при сериализации.
    blue_bans: Vec<u32>,
    red_bans: Vec<u32>,
    best_picks: Vec<PickSuggestion>,
    worst_picks: Vec<PickSuggestion>,
    build: Option<BuildRecommendation>,
    analytics: DraftAnalytics,
}

fn clamp_f32(x: f32, min: f32, max: f32) -> f32 {
    if !x.is_finite() {
        return (min + max) / 2.0;
    }
    x.clamp(min, max)
}

fn sanitize_analysis_result(r: &mut DraftAnalysisResult) {
    let a = &mut r.analytics;
    let clamp_opt = |v: Option<f32>| v.map(|x| clamp_f32(x, 0.0, 1.0));
    a.blue_win_probability = clamp_opt(a.blue_win_probability);
    a.red_win_probability = clamp_opt(a.red_win_probability);
    a.blue_synergy_score = clamp_opt(a.blue_synergy_score);
    a.red_synergy_score = clamp_opt(a.red_synergy_score);
    a.blue_damage_profile.ad = clamp_f32(a.blue_damage_profile.ad, 0.0, 1.0);
    a.blue_damage_profile.ap = clamp_f32(a.blue_damage_profile.ap, 0.0, 1.0);
    a.red_damage_profile.ad = clamp_f32(a.red_damage_profile.ad, 0.0, 1.0);
    a.red_damage_profile.ap = clamp_f32(a.red_damage_profile.ap, 0.0, 1.0);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SimulationEntry {
    champion_id: u32,
    champion_name: String,
    /// None до Этапа 2 — нет реальных данных для оценки.
    win_probability: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftSimulationResult {
    base_win_probability: Option<f32>,
    entries: Vec<SimulationEntry>,
}

/// Пустой драфт: LCU не найден или champ select сейчас не идёт.
/// Раньше тут возвращался мок с Ари/Зедом — теперь мок только для отладки (LOLDA_MOCK=1, debug-сборка).
fn empty_draft_state() -> DraftState {
    #[cfg(debug_assertions)]
    {
        if std::env::var("LOLDA_MOCK").as_deref() == Ok("1") {
            return mock_draft_state();
        }
    }
    let empty_team = |side: TeamSide| TeamDraft {
        side,
        slots: (0..5)
            .map(|_| DraftSlot {
                champion_id: None,
                champion_name: None,
                role: None,
                player_name: None,
            })
            .collect(),
        bans: Vec::new(),
    };
    DraftState {
        phase: Phase::None,
        blue: empty_team(TeamSide::Blue),
        red: empty_team(TeamSide::Red),
    }
}

#[cfg(debug_assertions)]
fn mock_draft_state() -> DraftState {
    DraftState {
        phase: Phase::ChampSelect,
        blue: TeamDraft {
            side: TeamSide::Blue,
            slots: vec![
                DraftSlot {
                    champion_id: Some(103),
                    champion_name: Some("Ahri".into()),
                    role: Some(Role::Mid),
                    player_name: Some("You".into()),
                },
                DraftSlot {
                    champion_id: Some(64),
                    champion_name: Some("Lee Sin".into()),
                    role: Some(Role::Jungle),
                    player_name: None,
                },
                DraftSlot {
                    champion_id: Some(51),
                    champion_name: Some("Caitlyn".into()),
                    role: Some(Role::Bot),
                    player_name: None,
                },
                DraftSlot {
                    champion_id: None,
                    champion_name: None,
                    role: Some(Role::Top),
                    player_name: None,
                },
                DraftSlot {
                    champion_id: None,
                    champion_name: None,
                    role: Some(Role::Support),
                    player_name: None,
                },
            ],
            bans: vec![157, 238, 777],
        },
        red: TeamDraft {
            side: TeamSide::Red,
            slots: vec![
                DraftSlot {
                    champion_id: Some(238),
                    champion_name: Some("Zed".into()),
                    role: Some(Role::Mid),
                    player_name: None,
                },
                DraftSlot {
                    champion_id: Some(157),
                    champion_name: Some("Yasuo".into()),
                    role: Some(Role::Top),
                    player_name: None,
                },
                DraftSlot {
                    champion_id: Some(222),
                    champion_name: Some("Jinx".into()),
                    role: Some(Role::Bot),
                    player_name: None,
                },
                DraftSlot {
                    champion_id: Some(412),
                    champion_name: Some("Thresh".into()),
                    role: Some(Role::Support),
                    player_name: None,
                },
                DraftSlot {
                    champion_id: None,
                    champion_name: None,
                    role: Some(Role::Jungle),
                    player_name: None,
                },
            ],
            bans: vec![103, 64, 523],
        },
    }
}

/// Профиль урона команды по рейтингам attack/magic из Data Dragon (0-10 на чемпиона).
/// Это дизайнерская оценка, не реальные доли урона — фронт подписывает её как оценку.
fn basic_damage_profile(team: &TeamDraft) -> DamageProfile {
    let mut ad = 0.0f32;
    let mut ap = 0.0f32;
    for slot in &team.slots {
        if let Some(id) = slot.champion_id {
            if let Some(meta) = ddragon::champion_meta(id) {
                ad += meta.attack as f32;
                ap += meta.magic as f32;
            }
        }
    }
    let total = (ad + ap).max(1.0);
    DamageProfile {
        ad: ad / total,
        ap: ap / total,
    }
}

fn detect_weaknesses(team: &TeamDraft, damage: &DamageProfile) -> Vec<String> {
    let mut res = Vec::new();
    if team
        .slots
        .iter()
        .all(|s| !matches!(s.role, Some(Role::Top) | Some(Role::Jungle)))
    {
        res.push("нет фронтлайна".to_string());
    }
    if damage.ad > 0.75 {
        res.push("слишком много AD урона".to_string());
    }
    if damage.ap > 0.75 {
        res.push("слишком много AP урона".to_string());
    }
    res
}

fn recommend_counter_items(draft: &DraftState, enemy_damage: &DamageProfile) -> Vec<BuildItem> {
    let mut items: Vec<BuildItem> = Vec::new();

    // Простые эвристики по типу урона.
    if enemy_damage.ad > 0.6 {
        items.push(BuildItem {
            id: 3047,
            name: "Plated Steelcaps".into(),
        });
        items.push(BuildItem {
            id: 3075,
            name: "Thornmail".into(),
        });
    }
    if enemy_damage.ap > 0.6 {
        items.push(BuildItem {
            id: 3102,
            name: "Banshee's Veil".into(),
        });
        items.push(BuildItem {
            id: 3157,
            name: "Zhonya's Hourglass".into(),
        });
    }

    // Если во вражеской команде есть ассасины (тег Assassin из Data Dragon),
    // добавляем Zhonya / защитные предметы.
    let has_assassin = draft.red.slots.iter().any(|s| {
        s.champion_id
            .and_then(ddragon::champion_meta)
            .map(|m| m.tags.iter().any(|t| t == "Assassin"))
            .unwrap_or(false)
    });
    if has_assassin {
        items.push(BuildItem {
            id: 3157,
            name: "Zhonya's Hourglass".into(),
        });
        items.push(BuildItem {
            id: 3026,
            name: "Guardian Angel".into(),
        });
    }

    // Убираем дубликаты по id.
    let mut seen: Vec<u32> = Vec::new();
    items
        .into_iter()
        .filter(|item| {
            if seen.contains(&item.id) {
                false
            } else {
                seen.push(item.id);
                true
            }
        })
        .collect()
}

fn enemy_comp_hash_from_draft(draft: &DraftState) -> Option<String> {
    // Считаем, что наша команда — blue, враги — red.
    let mut ids: Vec<i32> = draft
        .red
        .slots
        .iter()
        .filter_map(|s| s.champion_id.map(|id| id as i32))
        .collect();
    if ids.is_empty() {
        return None;
    }
    ids.sort_unstable();
    ids.dedup();
    Some(
        ids.into_iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join("-"),
    )
}

fn current_hero_champion(draft: &DraftState) -> Option<u32> {
    draft
        .blue
        .slots
        .iter()
        .filter_map(|s| s.champion_id)
        .next()
}

fn apply_candidate_pick(draft: &mut DraftState, champion_id: u32) {
    let name = lcu::champion_display_name(champion_id);
    if let Some(slot) = draft
        .blue
        .slots
        .iter_mut()
        .find(|s| s.champion_id.is_none())
    {
        slot.champion_id = Some(champion_id);
        slot.champion_name = Some(name);
    } else if let Some(slot) = draft.blue.slots.get_mut(0) {
        slot.champion_id = Some(champion_id);
        slot.champion_name = Some(name);
    }
}

fn analyze_draft(draft: DraftState) -> DraftAnalysisResult {
    let blue_damage = basic_damage_profile(&draft.blue);
    let red_damage = basic_damage_profile(&draft.red);

    let blue_weaknesses = detect_weaknesses(&draft.blue, &blue_damage);
    let red_weaknesses = detect_weaknesses(&draft.red, &red_damage);

    let counter_items = recommend_counter_items(&draft, &red_damage);

    // Вероятность победы и синергия требуют реальной статистики матчей — None до Этапа 2.
    let analytics = DraftAnalytics {
        blue_win_probability: None,
        red_win_probability: None,
        blue_synergy_score: None,
        red_synergy_score: None,
        blue_damage_profile: blue_damage,
        red_damage_profile: red_damage,
        blue_weaknesses,
        red_weaknesses,
    };

    // Пытаемся использовать статистику Emerald+ из локальной БД.
    let mut best_picks: Vec<PickSuggestion> = Vec::new();
    let mut worst_picks: Vec<PickSuggestion> = Vec::new();
    let mut build: Option<BuildRecommendation> = None;

    if let Some(enemy_hash) = enemy_comp_hash_from_draft(&draft) {
        if let Ok(db) = Database::open(&db_path()) {
            if let Ok(Some(patch_bucket)) =
                db.latest_patch_for_enemy_comp(&enemy_hash, db::RankBucket::EmeraldPlus)
            {
                if let Ok(top) = db.top_champions_vs_comp(
                    &enemy_hash,
                    &patch_bucket,
                    db::RankBucket::EmeraldPlus,
                    5,
                    5,
                ) {
                    best_picks = top
                        .into_iter()
                        .map(|agg| {
                            let wr = if agg.games > 0 {
                                agg.wins as f32 / agg.games as f32
                            } else {
                                0.0
                            };
                            PickSuggestion {
                                champion_id: agg.champion_id as u32,
                                champion_name: champion_display_name(agg.champion_id as u32),
                                winrate: wr,
                                games: agg.games as u32,
                                reason: "Наивысший винрейт против текущей вражеской команды (Emerald+).".into(),
                            }
                        })
                        .collect();
                }

                if let Ok(bottom) = db.bottom_champions_vs_comp(
                    &enemy_hash,
                    &patch_bucket,
                    db::RankBucket::EmeraldPlus,
                    5,
                    5,
                ) {
                    worst_picks = bottom
                        .into_iter()
                        .map(|agg| {
                            let wr = if agg.games > 0 {
                                agg.wins as f32 / agg.games as f32
                            } else {
                                0.0
                            };
                            PickSuggestion {
                                champion_id: agg.champion_id as u32,
                                champion_name: champion_display_name(agg.champion_id as u32),
                                winrate: wr,
                                games: agg.games as u32,
                                reason: "Низкий винрейт против текущей вражеской команды (Emerald+).".into(),
                            }
                        })
                        .collect();
                }

                if let Some(hero_champ) = current_hero_champion(&draft) {
                    if let Ok(build_aggs) =
                        db.top_builds(hero_champ as i32, Some(&enemy_hash), 3, 10)
                    {
                        if !build_aggs.is_empty() {
                            // Лучшая по винрейту — первая, самая популярная — с наибольшим games.
                            let mut by_popularity = build_aggs.clone();
                            by_popularity.sort_by_key(|b| std::cmp::Reverse(b.games));

                            let best_wr_build = &build_aggs[0];
                            let popular_build = &by_popularity[0];

                            // Имя предмета фронт не показывает — рендерит иконку
                            // Data Dragon по id, поэтому name пустой.
                            let parse_items = |hash: &str| -> Vec<BuildItem> {
                                hash.split('-')
                                    .filter_map(|part| part.parse::<u32>().ok())
                                    .map(|id| BuildItem { id, name: String::new() })
                                    .collect()
                            };

                            let best_winrate_items = parse_items(&best_wr_build.item_build_hash);
                            let most_popular_items = parse_items(&popular_build.item_build_hash);

                            build = Some(BuildRecommendation {
                                champion_id: hero_champ,
                                champion_name: champion_display_name(hero_champ),
                                best_winrate_build: best_winrate_items,
                                most_popular_build: most_popular_items,
                                vs_enemy_build: counter_items.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    // Если статистики в БД нет — списки остаются пустыми, build = None.
    // Фронт показывает «Недостаточно данных», а не выдуманные пики.
    // Контрпредметы по типу урона врага показываем и без БД (Data Dragon),
    // если враги уже что-то пикнули и есть кому их собирать.
    if build.is_none() && !counter_items.is_empty() {
        if let Some(hero_champ) = current_hero_champion(&draft) {
            build = Some(BuildRecommendation {
                champion_id: hero_champ,
                champion_name: champion_display_name(hero_champ),
                best_winrate_build: vec![],
                most_popular_build: vec![],
                vs_enemy_build: counter_items,
            });
        }
    }

    DraftAnalysisResult {
        draft: draft.clone(),
        blue_bans: draft.blue.bans.clone(),
        red_bans: draft.red.bans.clone(),
        best_picks,
        worst_picks,
        build,
        analytics,
    }
}

fn db_path() -> PathBuf {
    paths::db_path()
}

/// Только баны — отдельный вызов, не ломает отображение драфта.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DraftBansResponse {
    blue_bans: Vec<u32>,
    red_bans: Vec<u32>,
}

#[tauri::command]
async fn get_draft_bans() -> Result<DraftBansResponse, String> {
    tauri::async_runtime::spawn_blocking(|| {
        log_msg("get_draft_bans: start");
        match std::panic::catch_unwind(fetch_champ_select_state) {
            Ok(Some(draft)) => DraftBansResponse {
                blue_bans: draft.blue.bans.clone(),
                red_bans: draft.red.bans.clone(),
            },
            _ => DraftBansResponse {
                blue_bans: vec![],
                red_bans: vec![],
            },
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_draft_state() -> Result<DraftState, String> {
    tauri::async_runtime::spawn_blocking(|| {
        log_msg("get_draft_state: start");
        match std::panic::catch_unwind(|| fetch_champ_select_state().unwrap_or_else(empty_draft_state)) {
            Ok(state) => state,
            Err(_) => empty_draft_state(),
        }
    })
    .await
    .map_err(|e| e.to_string())
}

/// Ответ для экрана тир-листа: патчи, роли и строки таблицы (чемпион, роль, патч, игры, винрейт).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TierListResponse {
    patches: Vec<String>,
    roles: Vec<String>,
    rows: Vec<TierListRow>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TierListRow {
    champion_id: i32,
    champion_name: String,
    role: String,
    patch: String,
    games: i32,
    wins: i32,
    win_rate: f32,
}

#[tauri::command]
async fn get_tier_list(patch: Option<String>, role: Option<String>) -> Result<TierListResponse, String> {
    tauri::async_runtime::spawn_blocking(move || get_tier_list_blocking(patch, role))
        .await
        .map_err(|e| e.to_string())
}

fn get_tier_list_blocking(patch: Option<String>, role: Option<String>) -> TierListResponse {
    let patch = patch.unwrap_or_default();
    let role = role.unwrap_or_default();
    let path = db_path();
    match Database::open(&path) {
        Ok(db) => {
            let patches = db.get_tier_list_patches().unwrap_or_default();
            let roles = db.get_tier_list_roles().unwrap_or_default();
            let stats = db
                .get_champion_role_stats(&patch, &role, 1, 500)
                .unwrap_or_default();
            let rows: Vec<TierListRow> = stats
                .into_iter()
                .map(|s| {
                    let win_rate = if s.games > 0 {
                        (s.wins as f32) / (s.games as f32)
                    } else {
                        0.0
                    };
                    TierListRow {
                        champion_id: s.champion_id,
                        champion_name: lcu::champion_display_name(s.champion_id as u32),
                        role: s.role,
                        patch: s.patch,
                        games: s.games,
                        wins: s.wins,
                        win_rate,
                    }
                })
                .collect();
            TierListResponse { patches, roles, rows }
        }
        Err(_) => TierListResponse {
            patches: vec![],
            roles: vec![],
            rows: vec![],
        },
    }
}

#[tauri::command]
async fn analyze_current_draft() -> Result<DraftAnalysisResult, String> {
    tauri::async_runtime::spawn_blocking(|| {
        log_msg("analyze_current_draft: start");
        let mut result = match std::panic::catch_unwind(|| {
            let draft = fetch_champ_select_state().unwrap_or_else(empty_draft_state);
            analyze_draft(draft)
        }) {
            Ok(r) => r,
            Err(e) => {
                let msg = if let Some(s) = e.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = e.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "panic (unknown)".to_string()
                };
                log_msg(&format!("analyze_current_draft: PANIC caught: {}", msg));
                analyze_draft(empty_draft_state())
            }
        };
        sanitize_analysis_result(&mut result);
        result
    })
    .await
    .map_err(|e| e.to_string())
}

/// Каталог чемпионов для фронтенда: имена, иконки и версия Data Dragon.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampionCatalogResponse {
    version: String,
    champions: Vec<ddragon::ChampionMeta>,
}

#[tauri::command]
fn get_champion_catalog(state: tauri::State<'_, AppState>) -> ChampionCatalogResponse {
    let cat = state.catalog.read().unwrap();
    let mut champions: Vec<ddragon::ChampionMeta> = cat.by_id.values().cloned().collect();
    champions.sort_by(|a, b| a.name.cmp(&b.name));
    ChampionCatalogResponse {
        version: cat.version.clone(),
        champions,
    }
}

#[tauri::command]
fn get_league_path() -> String {
    lcu::get_league_path()
}

#[tauri::command]
fn set_league_path(path: String) -> Result<(), String> {
    lcu::set_league_path(path.trim())
}

#[tauri::command]
async fn check_lcu() -> Result<lcu::LcuCheckResult, String> {
    tauri::async_runtime::spawn_blocking(check_lcu_blocking)
        .await
        .map_err(|e| e.to_string())
}

fn check_lcu_blocking() -> lcu::LcuCheckResult {
    log_msg("check_lcu: start");
    match std::panic::catch_unwind(check_lcu_and_fetch_session) {
        Ok(result) => result,
        Err(e) => {
            let msg = if let Some(s) = e.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else {
                "Внутренняя ошибка при проверке LCU.".to_string()
            };
            lcu::LcuCheckResult {
                found: false,
                port: None,
                message: format!("Ошибка: {}", msg),
                session_saved: false,
            }
        }
    }
}

#[tauri::command]
async fn simulate_picks(champion_ids: Vec<u32>) -> Result<DraftSimulationResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        log_msg("simulate_picks: start");
        let base_draft = fetch_champ_select_state().unwrap_or_else(empty_draft_state);
        let base_result = analyze_draft(base_draft.clone());
        let base_win = base_result.analytics.blue_win_probability;

        let mut entries = Vec::new();

        for champ_id in champion_ids {
            let mut variant = base_draft.clone();
            apply_candidate_pick(&mut variant, champ_id);
            let result = analyze_draft(variant);
            entries.push(SimulationEntry {
                champion_id: champ_id,
                champion_name: champion_display_name(champ_id),
                win_probability: result.analytics.blue_win_probability,
            });
        }

        DraftSimulationResult {
            base_win_probability: base_win,
            entries,
        }
    })
    .await
    .map_err(|e| e.to_string())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentGamePlayer {
    summoner_name: String,
    riot_id: String,
    champion_id: u32,
    champion_name: String,
    rank: String,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentGameInfoResponse {
    has_game: bool,
    error_message: Option<String>,
    my_team: Vec<CurrentGamePlayer>,
    enemy_team: Vec<CurrentGamePlayer>,
}

#[tauri::command]
async fn get_current_game_info(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
) -> Result<CurrentGameInfoResponse, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || {
        get_current_game_info_blocking(limiter, api_key, region)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn get_current_game_info_blocking(
    limiter: SharedLimiter,
    api_key: String,
    region: String,
) -> Result<CurrentGameInfoResponse, String> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("Введите Riot API ключ в Настройках.".into());
    }
    let current = get_current_summoner().ok_or_else(|| {
        "LCU не найден или вы не авторизованы в клиенте. Запустите League of Legends и войдите в аккаунт.".to_string()
    })?;
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    // Регион берём из LCU (реальный регион аккаунта игрока), а НЕ из ручной
    // настройки: если они не совпадают, Spectator не расшифрует puuid на чужом
    // шарде и вернёт HTTP 400. Ручная настройка — только фолбэк.
    let region = get_current_platform().unwrap_or_else(|| region.trim().to_string());
    let cfg = RiotConfig { api_key, region };
    // Spectator-V5 принимает PUUID (LCU отдаёт его для текущего игрока).
    let game = match fetch_active_game(&client, &cfg, &limiter, &current.puuid) {
        Ok(None) => {
            return Ok(CurrentGameInfoResponse {
                has_game: false,
                error_message: None,
                my_team: vec![],
                enemy_team: vec![],
            });
        }
        Ok(Some(g)) => g,
        Err(riot_api::RiotError::Http(400)) => {
            return Err(format!(
                "Riot API 400: регион аккаунта ({}) не распознан или не совпал с puuid. Откройте клиент LoL, дождитесь полной загрузки и повторите.",
                cfg.region
            ));
        }
        Err(e) => return Err(e.into()),
    };
    let my_team_id = game
        .participants
        .iter()
        .find(|p| p.puuid == current.puuid)
        .map(|p| p.team_id)
        .unwrap_or(100);
    let mut my_team = Vec::new();
    let mut enemy_team = Vec::new();
    for p in &game.participants {
        let champ_name = champion_display_name(p.champion_id as u32);
        let rank = if p.puuid.is_empty() {
            "—".to_string()
        } else {
            fetch_league_entry_display(&client, &cfg, &limiter, &p.puuid)
                .unwrap_or_else(|_| "—".to_string())
        };
        let player = CurrentGamePlayer {
            summoner_name: p.summoner_name.clone(),
            riot_id: p.riot_id.clone(),
            champion_id: p.champion_id as u32,
            champion_name: champ_name,
            rank,
        };
        if p.team_id == my_team_id {
            my_team.push(player);
        } else {
            enemy_team.push(player);
        }
    }
    Ok(CurrentGameInfoResponse {
        has_game: true,
        error_message: None,
        my_team,
        enemy_team,
    })
}

/// Отладка: полный пайплайн Account-V1 → Spectator-V5 → League-V4 по произвольному
/// Riot ID (без LCU). Позволяет проверить API на стримере/челленджере, который сейчас в игре.
#[tauri::command]
async fn debug_game_info_for_riot_id(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    game_name: String,
    tag_line: String,
) -> Result<CurrentGameInfoResponse, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<CurrentGameInfoResponse, String> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err("Введите Riot API ключ.".into());
        }
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let cfg = RiotConfig {
            api_key,
            region: region.trim().to_string(),
        };
        let account = fetch_account_by_riot_id(&client, &cfg, &limiter, &game_name, &tag_line)
            .map_err(|e| match e {
                riot_api::RiotError::NotFound => format!(
                    "Riot ID {}#{} не найден (проверьте имя, тег и регион).",
                    game_name.trim(),
                    tag_line.trim()
                ),
                other => other.to_user_message(),
            })?;
        let game = match fetch_active_game(&client, &cfg, &limiter, &account.puuid)? {
            None => {
                return Ok(CurrentGameInfoResponse {
                    has_game: false,
                    error_message: Some(format!(
                        "Аккаунт {}#{} найден (PUUID получен), но активной игры сейчас нет.",
                        account.game_name, account.tag_line
                    )),
                    my_team: vec![],
                    enemy_team: vec![],
                });
            }
            Some(g) => g,
        };
        let target_team_id = game
            .participants
            .iter()
            .find(|p| p.puuid == account.puuid)
            .map(|p| p.team_id)
            .unwrap_or(100);
        let mut my_team = Vec::new();
        let mut enemy_team = Vec::new();
        for p in &game.participants {
            let rank = if p.puuid.is_empty() {
                "—".to_string()
            } else {
                fetch_league_entry_display(&client, &cfg, &limiter, &p.puuid)
                    .unwrap_or_else(|_| "—".to_string())
            };
            let player = CurrentGamePlayer {
                summoner_name: p.summoner_name.clone(),
                riot_id: p.riot_id.clone(),
                champion_id: p.champion_id as u32,
                champion_name: champion_display_name(p.champion_id as u32),
                rank,
            };
            if p.team_id == target_team_id {
                my_team.push(player);
            } else {
                enemy_team.push(player);
            }
        }
        Ok(CurrentGameInfoResponse {
            has_game: true,
            error_message: None,
            my_team,
            enemy_team,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- Этап 2: краулер ----------

#[tauri::command]
fn start_crawl(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    include_diamond: bool,
    max_matches: u32,
    reset: bool,
) -> Result<(), String> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("Введите Riot API ключ в Настройках.".into());
    }
    {
        let s = state.crawl.status.lock().unwrap();
        if s.running {
            return Err("Краулер уже запущен.".into());
        }
    }
    state.crawl.stop.store(false, std::sync::atomic::Ordering::Relaxed);
    // Текущий патч из версии каталога ("16.12.1" -> "16.12") — собираем только его.
    let current_patch = state
        .catalog
        .read()
        .ok()
        .map(|c| {
            let parts: Vec<&str> = c.version.split('.').collect();
            if parts.len() >= 2 { format!("{}.{}", parts[0], parts[1]) } else { c.version.clone() }
        });
    // Краулер работает с низким приоритетом: уступает профилю/текущей игре
    // и не занимает зарезервированные под интерактив слоты лимитера.
    let limiter = state.limiter.low();
    let stop = state.crawl.stop.clone();
    let status = state.crawl.status.clone();
    let region = region.trim().to_string();
    std::thread::spawn(move || {
        crawler::run_crawl(api_key, region, include_diamond, max_matches, reset, current_patch, limiter, stop, status);
    });
    Ok(())
}

#[tauri::command]
fn stop_crawl(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.crawl.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Ok(mut s) = state.crawl.status.lock() {
        s.message = "Останавливаю…".into();
    }
    Ok(())
}

#[tauri::command]
fn get_crawl_status(state: tauri::State<'_, AppState>) -> crawler::CrawlStatus {
    state.crawl.status.lock().unwrap().clone()
}

// ---------- Профиль игрока (живой, без хранения) ----------

#[tauri::command]
async fn fetch_profile(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    game_name: String,
    tag_line: String,
) -> Result<profile::ProfileResponse, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || {
        profile::fetch_profile_impl(&limiter, api_key, region, game_name, tag_line, 25)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- Разбор матча (Этап 1: счёт, руны, challenges) ----------

#[tauri::command]
async fn get_match_detail(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    match_id: String,
    target_puuid: Option<String>,
) -> Result<match_detail::MatchDetailResponse, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || {
        match_detail::fetch_match_detail_impl(
            &limiter,
            api_key,
            region,
            match_id,
            target_puuid.unwrap_or_default(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RuneIcon {
    id: i32,
    url: String,
    name: String,
    desc: String,
}

/// Каталог рун (ленивая загрузка + кэш): id → иконка/имя/описание.
#[tauri::command]
async fn get_rune_icons() -> Result<Vec<RuneIcon>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let to_icons =
            |map: &std::collections::HashMap<i32, ddragon::RuneInfo>| -> Vec<RuneIcon> {
                map.iter()
                    .map(|(id, info)| RuneIcon {
                        id: *id,
                        url: format!("https://ddragon.leagueoflegends.com/cdn/img/{}", info.icon),
                        name: info.name.clone(),
                        desc: info.desc.clone(),
                    })
                    .collect()
            };
        // Быстрый путь: уже в кэше.
        {
            let cache = ddragon::runes_cache().read().unwrap();
            if !cache.is_empty() {
                return to_icons(&cache);
            }
        }
        // Иначе тянем из сети для текущей версии каталога.
        let version = ddragon::current_version().unwrap_or_default();
        if !version.is_empty() {
            if let Ok(client) = Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
            {
                let map = ddragon::fetch_rune_catalog(&client, &version);
                if !map.is_empty() {
                    if let Ok(mut w) = ddragon::runes_cache().write() {
                        *w = map;
                    }
                }
            }
        }
        let cache = ddragon::runes_cache().read().unwrap();
        to_icons(&cache)
    })
    .await
    .map_err(|e| e.to_string())
}

// ---------- Champion-Mastery (мейны игрока) ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampionMasteryEntry {
    champion_id: u32,
    champion_name: String,
    level: i32,
    points: i64,
}

#[tauri::command]
async fn get_champion_mastery(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    puuid: String,
) -> Result<Vec<ChampionMasteryEntry>, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<ChampionMasteryEntry>, String> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err("Введите Riot API ключ в Настройках.".into());
        }
        if puuid.trim().is_empty() {
            return Err("Нет PUUID игрока.".into());
        }
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let cfg = RiotConfig {
            api_key,
            region: region.trim().to_string(),
        };
        let list = riot_api::fetch_champion_mastery(&client, &cfg, &limiter, puuid.trim())
            .map_err(|e| e.to_user_message())?;
        Ok(list
            .into_iter()
            .take(12)
            .map(|(id, lvl, pts)| ChampionMasteryEntry {
                champion_id: id as u32,
                champion_name: champion_display_name(id as u32),
                level: lvl,
                points: pts,
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- Рекомендации пиков (движок v1) ----------

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct PickArg {
    champion_id: i32,
    role: String,
}

/// Текущий патч-бакет из каталога Data Dragon ("16.12.1" → "16.12").
fn current_patch_bucket() -> String {
    ddragon::current_version()
        .map(|v| {
            let parts: Vec<&str> = v.split('.').collect();
            if parts.len() >= 2 {
                format!("{}.{}", parts[0], parts[1])
            } else {
                v
            }
        })
        .unwrap_or_default()
}

/// Патч для запроса статистики: явно переданный фронтом или (если пусто) текущий.
fn resolve_patch(arg: Option<String>) -> String {
    match arg {
        Some(p) if !p.trim().is_empty() => p,
        _ => current_patch_bucket(),
    }
}

/// Опции селектора патча для фронтенда: текущий и (если есть) предыдущий.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PatchOptions {
    current: String,
    previous: Option<String>,
}

#[tauri::command]
async fn get_patch_options() -> Result<PatchOptions, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<PatchOptions, String> {
        let current = current_patch_bucket();
        let previous = match Database::open(&db_path()) {
            Ok(db) => db.previous_patch(&current).unwrap_or(None),
            Err(_) => None,
        };
        Ok(PatchOptions { current, previous })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn get_pick_recommendations(
    my_role: String,
    enemies: Vec<PickArg>,
    allies: Vec<PickArg>,
    patch: Option<String>,
) -> Result<Vec<recommend::PickRec>, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<recommend::PickRec>, String> {
        let db = Database::open(&db_path())?;
        let patch = resolve_patch(patch);
        let input = recommend::PickInput {
            my_role,
            enemies: enemies.into_iter().map(|p| (p.champion_id, p.role)).collect(),
            allies: allies.into_iter().map(|p| (p.champion_id, p.role)).collect(),
        };
        recommend::recommend(&db, &input, 8, &patch)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Рекомендованная сборка для зафиксированного чемпиона локального игрока.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveBuildRec {
    /// Чемпион, под которого собрана сборка.
    champion_id: u32,
    champion_name: String,
    /// Роль, по которой подбиралась сборка.
    role: String,
    /// Стартовые/первые предметы (по частоте).
    first_items: Vec<ChampItem>,
    /// Рекомендованные ботинки (slot 0).
    boots: Vec<ChampItem>,
    /// Порядок сборки по слотам (1 = первый предмет, 2 = второй, …).
    build_path: Vec<BuildSlot>,
}

/// Результат живого подбора пика по данным чемпион-селекта из LCU.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveDraftRecommendations {
    /// Находимся ли мы сейчас в чемпион-селекте (LCU отдал сессию).
    in_champ_select: bool,
    /// Авто-определённая (или переопределённая) роль локального игрока; "" если неизвестна.
    my_role: String,
    recommendations: Vec<recommend::PickRec>,
    /// Сборка для зафиксированного чемпиона локального игрока; None, если не зафиксирован
    /// или нет данных по сборке для этой роли/патча.
    build: Option<LiveBuildRec>,
}

/// Живые рекомендации пиков по данным чемпион-селекта из клиента LoL (LCU).
/// `role_override` — ручной выбор роли, если авто-детект не сработал.
#[tauri::command]
async fn get_live_draft_recommendations(
    patch: Option<String>,
    role_override: Option<String>,
) -> Result<LiveDraftRecommendations, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<LiveDraftRecommendations, String> {
        // Нет LCU / не в чемпион-селекте → пустой результат, без ошибки.
        let (draft, auto_role) = match lcu::fetch_champ_select_live() {
            Some(pair) => pair,
            None => {
                return Ok(LiveDraftRecommendations {
                    in_champ_select: false,
                    my_role: String::new(),
                    recommendations: Vec::new(),
                    build: None,
                });
            }
        };

        // Роль: ручное переопределение приоритетнее авто-детекта.
        let my_role = match role_override {
            Some(r) if !r.trim().is_empty() => r.trim().to_string(),
            _ => auto_role.map(|r| r.as_engine_str().to_string()).unwrap_or_default(),
        };

        // Союзники = пики моей команды (blue) с известным чемпионом и ролью.
        let allies: Vec<(i32, String)> = draft
            .blue
            .slots
            .iter()
            .filter_map(|s| {
                let cid = s.champion_id? as i32;
                if cid <= 0 {
                    return None;
                }
                let role = s.role.as_ref().map(|r| r.as_engine_str().to_string()).unwrap_or_default();
                Some((cid, role))
            })
            .collect();

        // Враги = пики красной команды; роль врага в чемпион-селекте обычно неизвестна → "".
        let enemies: Vec<(i32, String)> = draft
            .red
            .slots
            .iter()
            .filter_map(|s| {
                let cid = s.champion_id? as i32;
                if cid <= 0 {
                    return None;
                }
                let role = s.role.as_ref().map(|r| r.as_engine_str().to_string()).unwrap_or_default();
                Some((cid, role))
            })
            .collect();

        // Зафиксированный чемпион локального игрока: слот моей команды (blue), чей role
        // совпадает с моей ролью и у которого уже выбран чемпион. Если совпадения по роли
        // нет, но в синей команде ровно один зафиксированный чемпион — берём его.
        let locked_champion: Option<i32> = {
            let by_role = draft.blue.slots.iter().find_map(|s| {
                let cid = s.champion_id? as i32;
                if cid <= 0 {
                    return None;
                }
                let role = s.role.as_ref().map(|r| r.as_engine_str()).unwrap_or("");
                if !my_role.is_empty() && role == my_role {
                    Some(cid)
                } else {
                    None
                }
            });
            by_role.or_else(|| {
                let locked: Vec<i32> = draft
                    .blue
                    .slots
                    .iter()
                    .filter_map(|s| {
                        let cid = s.champion_id? as i32;
                        if cid > 0 {
                            Some(cid)
                        } else {
                            None
                        }
                    })
                    .collect();
                if locked.len() == 1 {
                    Some(locked[0])
                } else {
                    None
                }
            })
        };

        // Без роли движок вернёт пусто — отдаём пустой список, но in_champ_select=true.
        if my_role.is_empty() {
            return Ok(LiveDraftRecommendations {
                in_champ_select: true,
                my_role,
                recommendations: Vec::new(),
                build: None,
            });
        }

        let db = Database::open(&db_path())?;
        let patch = resolve_patch(patch);
        let input = recommend::PickInput {
            my_role: my_role.clone(),
            enemies,
            allies,
        };
        let recommendations = recommend::recommend(&db, &input, 8, &patch)?;

        // Сборка для зафиксированного чемпиона (если он есть). Те же агрегаты, что на
        // странице «Чемпион»: первые предметы, ботинки и порядок сборки по слотам.
        let build = match locked_champion {
            Some(cid) => build_recommendation(&db, &patch, &my_role, cid)?,
            None => None,
        };

        Ok(LiveDraftRecommendations {
            in_champ_select: true,
            my_role,
            recommendations,
            build,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Собирает рекомендованную сборку для чемпиона в роли по агрегатам предметов.
/// Возвращает None, если по этой роли/патчу нет данных о сборке.
fn build_recommendation(
    db: &Database,
    patch: &str,
    role: &str,
    champion_id: i32,
) -> Result<Option<LiveBuildRec>, String> {
    let map_items = |rows: Vec<(i32, i64, i64)>| -> Vec<ChampItem> {
        rows.into_iter()
            .map(|(id, g, w)| ChampItem {
                item_id: id as u32,
                games: g,
                win_rate: wr_of(g, w),
            })
            .collect()
    };

    let first_items = map_items(db.champion_items(patch, role, champion_id, 1, 1, 6)?);
    let boots = map_items(db.champion_item_order(patch, role, champion_id, 0, 1, 3)?);
    let mut build_path: Vec<BuildSlot> = Vec::new();
    for slot in 1..=5 {
        let items = map_items(db.champion_item_order(patch, role, champion_id, slot, 1, 3)?);
        if !items.is_empty() {
            build_path.push(BuildSlot { slot, items });
        }
    }

    // Нет ни первых предметов, ни порядка сборки — данных недостаточно.
    if first_items.is_empty() && boots.is_empty() && build_path.is_empty() {
        return Ok(None);
    }

    Ok(Some(LiveBuildRec {
        champion_id: champion_id as u32,
        champion_name: champion_display_name(champion_id as u32),
        role: role.to_string(),
        first_items,
        boots,
        build_path,
    }))
}

// ---------- Страница «Чемпион» ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampMatchup {
    champion_id: u32,
    champion_name: String,
    games: i64,
    win_rate: f32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampSynergy {
    champion_id: u32,
    champion_name: String,
    role: String,
    games: i64,
    win_rate: f32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampItem {
    item_id: u32,
    games: i64,
    win_rate: f32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampRoleOpt {
    role: String,
    games: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampRune {
    rune_id: i32,
    games: i64,
    win_rate: f32,
}

/// Руны чемпиона: топ кейстоунов и древ (основное/вторичное).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampRunes {
    keystones: Vec<ChampRune>,
    primary: Vec<ChampRune>,
    secondary: Vec<ChampRune>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BuildSlot {
    /// Номер предмета в порядке сборки (1 = первый, 2 = второй, …).
    slot: i32,
    items: Vec<ChampItem>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ChampionPageResponse {
    found: bool,
    champion_id: u32,
    champion_name: String,
    role: String,
    roles: Vec<ChampRoleOpt>,
    games: i64,
    win_rate: f32,
    pick_rate: f32,
    ban_rate: f32,
    strong_against: Vec<ChampMatchup>,
    weak_against: Vec<ChampMatchup>,
    synergies: Vec<ChampSynergy>,
    first_items: Vec<ChampItem>,
    final_items: Vec<ChampItem>,
    boots: Vec<ChampItem>,
    build_path: Vec<BuildSlot>,
    runes: ChampRunes,
}

fn wr_of(games: i64, wins: i64) -> f32 {
    if games > 0 {
        wins as f32 / games as f32
    } else {
        0.0
    }
}

#[tauri::command]
async fn get_champion_page(
    champion_id: i32,
    role: Option<String>,
    patch: Option<String>,
) -> Result<ChampionPageResponse, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<ChampionPageResponse, String> {
        let db = Database::open(&db_path())?;
        let patch = resolve_patch(patch);
        let cid = champion_id as u32;
        let name = champion_display_name(cid);
        let roles = db.champion_roles(&patch, champion_id)?;
        if roles.is_empty() {
            return Ok(ChampionPageResponse {
                found: false,
                champion_id: cid,
                champion_name: name,
                role: String::new(),
                roles: vec![],
                games: 0,
                win_rate: 0.0,
                pick_rate: 0.0,
                ban_rate: 0.0,
                strong_against: vec![],
                weak_against: vec![],
                synergies: vec![],
                first_items: vec![],
                final_items: vec![],
                boots: vec![],
                build_path: vec![],
                runes: ChampRunes { keystones: vec![], primary: vec![], secondary: vec![] },
            });
        }
        let role_opts: Vec<ChampRoleOpt> = roles
            .iter()
            .map(|(r, g, _)| ChampRoleOpt { role: r.clone(), games: *g })
            .collect();
        let chosen = role
            .filter(|r| !r.trim().is_empty())
            .unwrap_or_else(|| roles[0].0.clone());
        let (games, wins) = roles
            .iter()
            .find(|(r, _, _)| *r == chosen)
            .map(|(_, g, w)| (*g, *w))
            .unwrap_or((0, 0));

        let total = db.total_matches(&patch)?.max(1);
        let champ_games = db.champion_total_games(&patch, champion_id)?;
        let bans = db.champion_ban_games(&patch, champion_id)?;

        // Матчапы → сильные/слабые.
        let mut strong: Vec<ChampMatchup> = Vec::new();
        let mut weak: Vec<ChampMatchup> = Vec::new();
        for (eid, g, w) in db.champion_matchups(&patch, &chosen, champion_id, 1, 60)? {
            let wr = wr_of(g, w);
            let m = ChampMatchup {
                champion_id: eid as u32,
                champion_name: champion_display_name(eid as u32),
                games: g,
                win_rate: wr,
            };
            if wr >= 0.5 {
                strong.push(m);
            } else {
                weak.push(m);
            }
        }
        strong.sort_by(|a, b| b.win_rate.partial_cmp(&a.win_rate).unwrap_or(std::cmp::Ordering::Equal));
        weak.sort_by(|a, b| a.win_rate.partial_cmp(&b.win_rate).unwrap_or(std::cmp::Ordering::Equal));
        strong.truncate(8);
        weak.truncate(8);

        let synergies: Vec<ChampSynergy> = db
            .champion_synergies(&patch, &chosen, champion_id, 1, 12)?
            .into_iter()
            .map(|(arole, aid, g, w)| ChampSynergy {
                champion_id: aid as u32,
                champion_name: champion_display_name(aid as u32),
                role: arole,
                games: g,
                win_rate: wr_of(g, w),
            })
            .collect();

        let map_items = |rows: Vec<(i32, i64, i64)>| -> Vec<ChampItem> {
            rows.into_iter()
                .map(|(id, g, w)| ChampItem {
                    item_id: id as u32,
                    games: g,
                    win_rate: wr_of(g, w),
                })
                .collect()
        };
        let first_items = map_items(db.champion_items(&patch, &chosen, champion_id, 1, 1, 8)?);
        let final_items = map_items(db.champion_items(&patch, &chosen, champion_id, 0, 1, 12)?);

        // Путь сборки: ботинки (slot 0) + предметы по порядку (slot 1..5).
        let boots = map_items(db.champion_item_order(&patch, &chosen, champion_id, 0, 1, 4)?);
        let mut build_path: Vec<BuildSlot> = Vec::new();
        for slot in 1..=5 {
            let items = map_items(db.champion_item_order(&patch, &chosen, champion_id, slot, 1, 4)?);
            if !items.is_empty() {
                build_path.push(BuildSlot { slot, items });
            }
        }

        // Руны: топ кейстоунов и древ для выбранной роли.
        let mut keystones: Vec<ChampRune> = Vec::new();
        let mut primary: Vec<ChampRune> = Vec::new();
        let mut secondary: Vec<ChampRune> = Vec::new();
        for r in db.champion_runes(&patch, &chosen, champion_id, 1, 4)? {
            let item = ChampRune {
                rune_id: r.rune_id,
                games: r.games,
                win_rate: wr_of(r.games, r.wins),
            };
            match r.kind.as_str() {
                "keystone" => keystones.push(item),
                "primary" => primary.push(item),
                "secondary" => secondary.push(item),
                _ => {}
            }
        }

        Ok(ChampionPageResponse {
            found: true,
            champion_id: cid,
            champion_name: name,
            role: chosen,
            roles: role_opts,
            games,
            win_rate: wr_of(games, wins),
            pick_rate: champ_games as f32 / total as f32,
            ban_rate: bans as f32 / total as f32,
            strong_against: strong,
            weak_against: weak,
            synergies,
            first_items,
            final_items,
            boots,
            build_path,
            runes: ChampRunes { keystones, primary, secondary },
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- Мета тир-лист (из данных краулера) ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MetaTierRow {
    champion_id: i32,
    champion_name: String,
    role: String,
    patch: String,
    games: i64,
    wins: i64,
    win_rate: f32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MetaTierResponse {
    patches: Vec<String>,
    roles: Vec<String>,
    rows: Vec<MetaTierRow>,
}

#[tauri::command]
async fn get_meta_tier_list(
    patch: Option<String>,
    role: Option<String>,
) -> Result<MetaTierResponse, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<MetaTierResponse, String> {
        let db = Database::open(&db_path())?;
        let patches = db.meta_tier_patches().unwrap_or_default();
        let roles = db.meta_tier_roles().unwrap_or_default();
        let rows = db
            .meta_tier_list(&patch.unwrap_or_default(), &role.unwrap_or_default(), 10, 300)?
            .into_iter()
            .map(|r| {
                let win_rate = if r.games > 0 { r.wins as f32 / r.games as f32 } else { 0.0 };
                MetaTierRow {
                    champion_id: r.champion_id,
                    champion_name: champion_display_name(r.champion_id as u32),
                    role: r.role,
                    patch: r.patch,
                    games: r.games,
                    wins: r.wins,
                    win_rate,
                }
            })
            .collect();
        Ok(MetaTierResponse { patches, roles, rows })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- Скаут: разведка чужой активной игры ----------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScoutPlayer {
    puuid: String,
    riot_id: String,
    summoner_name: String,
    champion_id: u32,
    champion_name: String,
    team_id: i32,
    spell1_id: i32,
    spell2_id: i32,
    /// Теги чемпиона из Data Dragon (для авто-определения роли на фронте).
    champion_tags: Vec<String>,
    tier: String,
    rank: String,
    league_points: i32,
    wins: i32,
    losses: i32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ScoutResponse {
    has_game: bool,
    error_message: Option<String>,
    players: Vec<ScoutPlayer>,
}

/// Ядро скаута: по PUUID игрока (любой команды) тянет активную игру и собирает
/// обе команды с рангами. `not_in_game_msg` — текст, если активной игры нет.
/// Используется и ручным скаутом по Riot ID, и авто-скаутом своей игры через LCU.
fn scout_by_puuid(
    limiter: &SharedLimiter,
    cfg: &RiotConfig,
    client: &Client,
    puuid: &str,
    not_in_game_msg: String,
) -> Result<ScoutResponse, String> {
    let game = match fetch_active_game(client, cfg, limiter, puuid)? {
        None => {
            return Ok(ScoutResponse {
                has_game: false,
                error_message: Some(not_in_game_msg),
                players: vec![],
            })
        }
        Some(g) => g,
    };
    let mut players = Vec::new();
    for p in &game.participants {
        let rank = if p.puuid.is_empty() {
            None
        } else {
            riot_api::fetch_league_rank(client, cfg, limiter, &p.puuid).unwrap_or(None)
        };
        let (tier, rank_div, lp, wins, losses) = match rank {
            Some(r) => (r.tier, r.rank, r.league_points, r.wins, r.losses),
            None => (String::new(), String::new(), 0, 0, 0),
        };
        let tags = ddragon::champion_meta(p.champion_id as u32)
            .map(|m| m.tags)
            .unwrap_or_default();
        players.push(ScoutPlayer {
            puuid: p.puuid.clone(),
            riot_id: p.riot_id.clone(),
            summoner_name: p.summoner_name.clone(),
            champion_id: p.champion_id as u32,
            champion_name: champion_display_name(p.champion_id as u32),
            team_id: p.team_id,
            spell1_id: p.spell1_id,
            spell2_id: p.spell2_id,
            champion_tags: tags,
            tier,
            rank: rank_div,
            league_points: lp,
            wins,
            losses,
        });
    }
    Ok(ScoutResponse {
        has_game: true,
        error_message: None,
        players,
    })
}

#[tauri::command]
async fn scout_active_game(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    game_name: String,
    tag_line: String,
) -> Result<ScoutResponse, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<ScoutResponse, String> {
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
        let account = fetch_account_by_riot_id(&client, &cfg, &limiter, &game_name, &tag_line)
            .map_err(|e| match e {
                riot_api::RiotError::NotFound => format!(
                    "Riot ID {}#{} не найден (проверьте имя, тег и регион).",
                    game_name.trim(),
                    tag_line.trim()
                ),
                other => other.to_user_message(),
            })?;
        let not_in_game = format!(
            "{}#{} сейчас не в игре. Spectator видит игру через 1-2 минуты после её начала.",
            account.game_name, account.tag_line
        );
        scout_by_puuid(&limiter, &cfg, &client, &account.puuid, not_in_game)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Авто-скаут собственной активной игры: PUUID берётся из LCU (кто залогинен
/// в клиенте), затем тот же пайплайн, что и ручной скаут по Riot ID.
#[tauri::command]
async fn scout_my_active_game(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
) -> Result<ScoutResponse, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<ScoutResponse, String> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err("Введите Riot API ключ в Настройках.".into());
        }
        let current = get_current_summoner().ok_or_else(|| {
            "LCU не найден или вы не авторизованы в клиенте. Запустите League of Legends и войдите в аккаунт.".to_string()
        })?;
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let cfg = RiotConfig {
            api_key,
            region: region.trim().to_string(),
        };
        scout_by_puuid(
            &limiter,
            &cfg,
            &client,
            &current.puuid,
            "Вы сейчас не в игре. Spectator видит игру через 1-2 минуты после её начала.".to_string(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerGame {
    champion_id: u32,
    champion_name: String,
    win: bool,
    queue_id: i32,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerHistoryResponse {
    games: Vec<PlayerGame>,
}

#[tauri::command]
async fn scout_player_history(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    puuid: String,
    count: u32,
) -> Result<PlayerHistoryResponse, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<PlayerHistoryResponse, String> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err("Введите Riot API ключ в Настройках.".into());
        }
        if puuid.trim().is_empty() {
            return Err("Нет PUUID игрока.".into());
        }
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let cfg = RiotConfig {
            api_key,
            region: region.trim().to_string(),
        };
        let ids = riot_api::fetch_recent_match_ids(
            &client,
            &cfg,
            &limiter,
            puuid.trim(),
            count.clamp(1, 15) as usize,
        )?;
        let mut games = Vec::new();
        for id in ids {
            let parsed = match fetch_match(&client, &cfg, &limiter, &id) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if let Some(me) = parsed.participants.iter().find(|pp| pp.puuid == puuid.trim()) {
                games.push(PlayerGame {
                    champion_id: me.champion_id as u32,
                    champion_name: champion_display_name(me.champion_id as u32),
                    win: me.win,
                    queue_id: parsed.queue_id,
                });
            }
        }
        Ok(PlayerHistoryResponse { games })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Личный матчап игрока: на его текущем чемпионе против конкретного вражеского
/// чемпиона — по его собственной истории игр.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PersonalMatchupResponse {
    /// Сколько игр на этом чемпионе против этого врага найдено.
    games: u32,
    /// Сколько из них выиграно.
    wins: u32,
    /// Сколько недавних матчей просмотрено (знаменатель «редкости» выборки).
    scanned: u32,
}

/// Винрейт игрока на чемпионе `champion_id` против вражеского `enemy_champion_id`
/// по его последним матчам. Дорого: до `count` запросов полного матча — вызывать
/// по явному действию пользователя, не авто.
#[tauri::command]
async fn scout_personal_matchup(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    puuid: String,
    champion_id: i32,
    enemy_champion_id: i32,
    count: u32,
) -> Result<PersonalMatchupResponse, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<PersonalMatchupResponse, String> {
        let api_key = api_key.trim().to_string();
        if api_key.is_empty() {
            return Err("Введите Riot API ключ в Настройках.".into());
        }
        let puuid = puuid.trim().to_string();
        if puuid.is_empty() {
            return Err("Нет PUUID игрока.".into());
        }
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;
        let cfg = RiotConfig {
            api_key,
            region: region.trim().to_string(),
        };
        let ids = riot_api::fetch_recent_match_ids(
            &client,
            &cfg,
            &limiter,
            &puuid,
            count.clamp(1, 40) as usize,
        )?;
        let mut games = 0u32;
        let mut wins = 0u32;
        let mut scanned = 0u32;
        for id in ids {
            let parsed = match fetch_match(&client, &cfg, &limiter, &id) {
                Ok(m) => m,
                Err(_) => continue,
            };
            scanned += 1;
            let me = match parsed.participants.iter().find(|p| p.puuid == puuid) {
                Some(p) => p,
                None => continue,
            };
            if me.champion_id != champion_id {
                continue;
            }
            // Был ли в этом матче вражеский чемпион на другой команде.
            let faced = parsed
                .participants
                .iter()
                .any(|p| p.team_id != me.team_id && p.champion_id == enemy_champion_id);
            if faced {
                games += 1;
                if me.win {
                    wins += 1;
                }
            }
        }
        Ok(PersonalMatchupResponse { games, wins, scanned })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Мета-матчап champion vs enemy из агрегата краулера (мгновенно, без сети/лимита).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct MetaMatchupResponse {
    games: i64,
    wins: i64,
}

/// Агрегированный винрейт чемпиона против вражеского чемпиона в роли за патч
/// из локальной БД краулера. Мгновенный (без обращения к Riot API).
#[tauri::command]
async fn scout_meta_matchup(
    role: String,
    champion_id: i32,
    enemy_champion_id: i32,
    patch: Option<String>,
) -> Result<MetaMatchupResponse, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<MetaMatchupResponse, String> {
        let db = Database::open(&db_path())?;
        let patch = resolve_patch(patch);
        let (games, wins) = db.matchup_winrate(&patch, role.trim(), champion_id, enemy_champion_id)?;
        Ok(MetaMatchupResponse { games, wins })
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---------- паттерны игрока (предсказание поведения по истории) ----------

/// Анализ паттернов поведения врага по его истории матчей.
/// Дорого: ~2 запроса на игру (полный матч + таймлайн). Кэшируется по puuid в БД
/// на 12 часов, вызывать по явному действию пользователя («Анализ»), не авто.
#[tauri::command]
async fn scout_player_patterns(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    puuid: String,
    count: u32,
    force: Option<bool>,
    champion_id: Option<u32>,
    role: Option<String>,
) -> Result<archetypes::PlayerPatterns, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<archetypes::PlayerPatterns, String> {
        let puuid_t = puuid.trim().to_string();
        if puuid_t.is_empty() {
            return Err("Нет PUUID игрока.".into());
        }
        // Ключ кэша включает фильтр (чемпион+роль), чтобы фильтрованный и полный
        // результаты не затирали друг друга. 0/"" = без соответствующего фильтра.
        let cache_key = format!(
            "{}|{}|{}",
            puuid_t,
            champion_id.unwrap_or(0),
            role.as_deref().unwrap_or("")
        );
        // Кэш на 12 часов, если не запрошен принудительный пересчёт.
        const CACHE_TTL_SECS: i64 = 12 * 60 * 60;
        if force != Some(true) {
            if let Ok(db) = Database::open(&db_path()) {
                if let Ok(Some(payload)) = db.get_player_pattern_cache(&cache_key, CACHE_TTL_SECS) {
                    if let Ok(parsed) =
                        serde_json::from_str::<archetypes::PlayerPatterns>(&payload)
                    {
                        return Ok(parsed);
                    }
                }
            }
        }
        let patterns = archetypes::compute_player_patterns(
            &limiter,
            &api_key,
            &region,
            &puuid_t,
            count.clamp(1, 8),
            champion_id,
            role.as_deref(),
        )?;
        // Пишем в кэш (ошибки кэша не фатальны).
        if let Ok(db) = Database::open(&db_path()) {
            if let Ok(payload) = serde_json::to_string(&patterns) {
                let _ = db.put_player_pattern_cache(
                    &cache_key,
                    patterns.games_analyzed as i32,
                    &payload,
                );
            }
        }
        Ok(patterns)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn sync_matches(
    state: tauri::State<'_, AppState>,
    api_key: String,
    region: String,
    game_name: String,
    tag_line: String,
    count: u32,
) -> Result<u32, String> {
    let limiter = state.limiter.clone();
    tauri::async_runtime::spawn_blocking(move || {
        sync_matches_blocking(limiter, api_key, region, game_name, tag_line, count)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn sync_matches_blocking(
    limiter: SharedLimiter,
    api_key: String,
    region: String,
    game_name: String,
    tag_line: String,
    count: u32,
) -> Result<u32, String> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("Введите Riot API ключ в Настройках.".into());
    }
    if game_name.trim().is_empty() || tag_line.trim().is_empty() {
        return Err("Введите Riot ID в формате Имя#TAG в Настройках.".into());
    }
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| e.to_string())?;
    let cfg = RiotConfig { api_key, region };
    let account = fetch_account_by_riot_id(&client, &cfg, &limiter, &game_name, &tag_line)
        .map_err(|e| match e {
            riot_api::RiotError::NotFound => format!(
                "Riot ID {}#{} не найден. Проверьте имя и тег в Настройках.",
                game_name.trim(),
                tag_line.trim()
            ),
            other => other.to_user_message(),
        })?;
    let puuid = account.puuid.clone();

    // Проверяем, что аккаунт действительно Emerald+
    let is_emerald_plus = fetch_is_emerald_plus(&client, &cfg, &limiter, &puuid)?;
    if !is_emerald_plus {
        return Err("Ранг аккаунта ниже Emerald в соло-очереди; статистика не синхронизируется.".into());
    }

    let ids = fetch_match_ids(&client, &cfg, &limiter, &puuid, count as usize, None)?;

    let db = Database::open(&db_path())?;
    let mut processed = 0u32;

    for id in ids {
        let parsed = match fetch_match(&client, &cfg, &limiter, &id) {
            Ok(m) => m,
            Err(_) => continue,
        };

        if parsed.queue_id != 420 {
            continue;
        }

        db.insert_match(&parsed.match_id, &parsed.patch, parsed.queue_id)?;

        // Находим участника-героя (наш puuid)
        let hero_opt = parsed
            .participants
            .iter()
            .find(|p| p.puuid == puuid);
        let hero = match hero_opt {
            Some(h) => h,
            None => continue,
        };

        let enemy_hash = build_enemy_comp_hash(&parsed.participants, hero.team_id);

        let champ_agg = ChampionVsCompAgg {
            champion_id: hero.champion_id,
            enemy_comp_hash: enemy_hash.clone(),
            patch_bucket: parsed.patch.clone(),
            rank_bucket: db::RankBucket::EmeraldPlus,
            games: 1,
            wins: if hero.win { 1 } else { 0 },
        };
        db.upsert_champion_vs_comp(&champ_agg)?;

        if let Some(item_hash) = build_item_hash(hero) {
            let build_agg = BuildAgg {
                champion_id: hero.champion_id,
                enemy_comp_hash: Some(enemy_hash),
                item_build_hash: item_hash,
                games: 1,
                wins: if hero.win { 1 } else { 0 },
            };
            db.upsert_build(&build_agg)?;
        }

        // Сохраняем участников матча (хотя бы наш аккаунт с меткой Emerald+)
        for p in &parsed.participants {
            let rank_tier = if p.puuid == puuid {
                "EMERALD_PLUS"
            } else {
                "UNKNOWN"
            };
            let items = [p.item0, p.item1, p.item2, p.item3, p.item4, p.item5, p.item6];
            db.insert_participant(
                &parsed.match_id,
                &p.puuid,
                p.team_id,
                p.champion_id,
                Some(p.individual_position.as_str()),
                Some(p.lane.as_str()),
                p.win,
                rank_tier,
                items,
            )?;
        }

        processed += 1;
    }

    Ok(processed)
}

fn main() {
    // Перехват паники: пишем в лог до выхода, чтобы понять причину закрытия.
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let msg = if let Some(s) = info.payload().downcast_ref::<&str>() {
            format!("PANIC: {}", s)
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            format!("PANIC: {}", s)
        } else {
            "PANIC: (unknown)".to_string()
        };
        log_crash(&msg);
        if let Some(loc) = info.location() {
            log_crash(&format!("  at {}:{}:{}", loc.file(), loc.line(), loc.column()));
        }
        default_hook(info);
    }));

    match tauri::Builder::default()
        .manage(AppState {
            limiter: SharedLimiter::new(),
            // На старте — вшитый снапшот; кэш и сеть подтянутся в setup().
            catalog: Arc::new(std::sync::RwLock::new(ddragon::bundled())),
            crawl: crawler::CrawlControl::new(),
        })
        .setup(|app| {
            use tauri::Manager;
            match app.path().app_data_dir() {
                Ok(dir) => {
                    paths::init(dir);
                    paths::migrate_legacy_files();
                    log_msg(&format!("setup: data dir = {}", paths::data_dir().display()));
                }
                Err(e) => log_crash(&format!("setup: app_data_dir failed: {}", e)),
            }

            let catalog = app.state::<AppState>().catalog.clone();
            // Кэш с диска (если свежее снапшота — просто перезапишет).
            if let Some(cached) = ddragon::load_from_cache(&paths::ddragon_cache_dir()) {
                if let Ok(mut c) = catalog.write() {
                    *c = cached;
                }
            }
            ddragon::set_global(catalog.clone());

            // Фоновое обновление каталога из сети — не блокирует старт.
            let bg = catalog;
            tauri::async_runtime::spawn_blocking(move || {
                let client = match Client::builder()
                    .timeout(std::time::Duration::from_secs(20))
                    .build()
                {
                    Ok(c) => c,
                    Err(_) => return,
                };
                match ddragon::refresh_from_network(&client, &paths::ddragon_cache_dir()) {
                    Ok(fresh) => {
                        let ver = fresh.version.clone();
                        if let Ok(mut c) = bg.write() {
                            *c = fresh;
                        }
                        log_msg(&format!("ddragon: каталог обновлён до {}", ver));
                    }
                    Err(e) => log_msg(&format!("ddragon: обновление не удалось: {}", e)),
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_draft_state,
            get_draft_bans,
            analyze_current_draft,
            get_tier_list,
            get_champion_catalog,
            get_league_path,
            set_league_path,
            get_current_game_info,
            debug_game_info_for_riot_id,
            scout_active_game,
            scout_my_active_game,
            scout_player_history,
            scout_personal_matchup,
            scout_meta_matchup,
            scout_player_patterns,
            start_crawl,
            stop_crawl,
            get_crawl_status,
            fetch_profile,
            get_match_detail,
            get_rune_icons,
            get_champion_mastery,
            get_pick_recommendations,
            get_live_draft_recommendations,
            get_champion_page,
            get_patch_options,
            get_meta_tier_list,
            sync_matches,
            simulate_picks,
            check_lcu
        ])
        .run(tauri::generate_context!())
    {
        Ok(_) => log_msg("main: exit normal"),
        Err(e) => {
            log_crash(&format!("tauri run error: {}", e));
            panic!("error while running Vantage Draft Assistant: {}", e);
        }
    }
}

