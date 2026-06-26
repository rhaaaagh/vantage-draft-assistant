//! движок «паттернов игрока».
//!
//! По истории матчей (НЕ live: публичный API не даёт позиций врага в реальном
//! времени) выводим поведенческие метки в стиле porofessor: «Инвейдер · в 30%
//! игр берёт/ассистит убийство в чужом лесу в первые ~3 мин» и т. п.
//!
//! Источник — `CHAMPION_KILL` события (killerId + assists + position +
//! timestamp) и позиции из `participantFrames` за последние N игр. Расчёт
//! тяжёлый (по 1 запросу матча + таймлайна на игру), поэтому результат
//! кэшируется по puuid в `player_pattern_cache` и считается лениво, по явному
//! действию пользователя.

use crate::ddragon::display_name;
use crate::riot_api::{
    fetch_match_full, fetch_match_timeline_full, fetch_recent_match_ids, RiotConfig, SharedLimiter,
};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

/// Карта SR ~0..MAP_MAX по обеим осям. Совпадает с DeathMap во фронте.
const MAP_MAX: f32 = 15000.0;
/// Сколько последних матчей анализируем максимум (кап на rate limit).
const MAX_GAMES: usize = 8;
/// При фильтре по чемпиону/роли совпадений среди последних игр мало, поэтому
/// сканируем более глубокий пул match id и отбираем подходящие (кап на rate limit).
const SCAN_CAP: usize = 30;
/// Окно «раннего инвейда» в мс (≈ первые 3:15).
const EARLY_INVADE_MS: i64 = 195_000;
/// Размер бина тепловой карты позиций (минут). 1 минута: на каждом кадре
/// слайдера — максимум по одной точке на игру (≈8), а не сотни к поздней игре.
const HEATMAP_BIN_MIN: i64 = 1;

/// Одна точка на карте (доля 0..1 по каждой оси, y уже не инвертирован —
/// фронт сам решает, как рисовать; используем сырые игровые координаты в долях).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapPoint {
    /// Доля по X (0..1), 0 = низ-лево синей базы.
    pub x: f32,
    /// Доля по Y (0..1), 0 = низ карты.
    pub y: f32,
    /// Игровая минута события (для биннинга/слайдера).
    pub minute: i64,
    /// Точное игровое время события в секундах (для таймингов смертей M:SS).
    /// serde(default) — старые записи кэша без поля десериализуются как 0.
    #[serde(default)]
    pub at_seconds: i64,
}

/// Поведенческая метка с числом и пояснением (porofessor-style).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Archetype {
    /// Короткий заголовок: «Инвейдер», «Ганкер», «Фармер»…
    pub label: String,
    /// Доля игр 0..1, к которой относится метка (для «в N% игр…»).
    pub value: f32,
    /// Человекочитаемое пояснение.
    pub explanation: String,
}

/// Точки позиций игрока в одном временно́м бине (для хитмапа по минутам).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatmapBin {
    /// Нижняя граница бина в минутах (0,5,10,…).
    pub from_minute: i64,
    pub points: Vec<MapPoint>,
}

/// Реконструкция лесного маршрута для одной игры.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JungleRoute {
    /// "BLUE" | "RED" — сторона леса, где начат фарм (по первой позиции).
    pub start_side: String,
    /// Первые позиции лесника по минутам (порядок клира) для отрисовки маршрута.
    pub path: Vec<MapPoint>,
}

/// Полный результат анализа паттернов игрока (то, что кэшируется и уходит во фронт).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPatterns {
    pub puuid: String,
    /// Сколько игр реально проанализировано.
    pub games_analyzed: u32,
    /// Основная роль игрока по выборке (TOP/JUNGLE/MID/BOTTOM/UTILITY или "").
    pub main_role: String,
    /// Чаще всего играемый чемпион (для подписи).
    pub main_champion_id: u32,
    pub main_champion_name: String,
    pub archetypes: Vec<Archetype>,
    /// Точки гибели игрока по всем играм (для карты смертей).
    pub death_points: Vec<MapPoint>,
    /// Тепловая карта позиций игрока по 5-минутным бинам.
    pub heatmap: Vec<HeatmapBin>,
    /// Лесные маршруты по проанализированным играм (пусто, если не лесник).
    /// Сохранён для совместимости/переиспользования; фронт рисует `avg_jungle_route`.
    pub jungle_routes: Vec<JungleRoute>,
    /// ОДИН усреднённый «типичный» ранний лесной маршрут: средняя позиция игрока
    /// по ранним минутам, упорядоченная по времени. Пусто, если не лесник.
    pub avg_jungle_route: Vec<MapPoint>,
    /// Доля лесных игр, начатых в своём лесу (0..1). Остальное — инвейд/чужой старт.
    pub own_start_fraction: f32,
    /// ПОЛНЫЕ треки позиций по каждой игре (упорядочены по минуте) — для рисования
    /// точного маршрута перемещений «откуда-куда» линиями, по игре (не усреднённо).
    /// serde(default) — старые записи кэша без поля десериализуются как пустые.
    #[serde(default)]
    pub position_routes: Vec<Vec<MapPoint>>,
}

/// Нормализует игровые координаты (0..15000) в доли (0..1). Координаты вне
/// карты/нулевые отбрасываем выше по стеку.
fn to_point(x: i32, y: i32, at_seconds: i64) -> MapPoint {
    MapPoint {
        x: (x as f32 / MAP_MAX).clamp(0.0, 1.0),
        y: (y as f32 / MAP_MAX).clamp(0.0, 1.0),
        minute: at_seconds / 60,
        at_seconds,
    }
}

/// Лесные квадранты карты SR (грубо). Возвращает сторону леса для позиции
/// в первые секунды игры: ниже главной диагонали (x+y < MAP) — обычно нижняя
/// половина (синий нижний / красный верхний лес). Для «стартовой стороны»
/// нам важно лишь blue-half vs red-half относительно команды.
fn side_for_position(x: i32, y: i32, blue_team: bool) -> String {
    // Диагональ реки идёт из верх-лево в низ-право (как в DeathMap: line 0,S→S,0).
    // Точки ниже диагонали (x + y < MAP) — ближе к синей базе.
    let near_blue = (x as f32 + y as f32) < MAP_MAX;
    // «Своя» сторона леса: для синей команды это near_blue, для красной — наоборот.
    let own_side = near_blue == blue_team;
    if own_side {
        "OWN".to_string()
    } else {
        "ENEMY".to_string()
    }
}

/// Промежуточные данные одной игры для агрегации.
struct GamePatterns {
    role: String,
    champion_id: u32,
    /// Ранний инвейд: брал/ассистил килл в чужом лесу в первые ~3 мин.
    early_invade: bool,
    /// Первая кровь или участие в килле до 5:00.
    early_kill_part: bool,
    /// Сколько раз погиб всего.
    deaths: u32,
    /// Точки гибели игрока.
    death_points: Vec<MapPoint>,
    /// Позиции игрока по кадрам (минуты + координаты).
    positions: Vec<MapPoint>,
    /// Лесной маршрут (если лесник).
    jungle_route: Option<JungleRoute>,
}

/// Приводит роль к канону TOP/JUNGLE/MID/BOT/SUPPORT: Riot team_position
/// (MIDDLE/BOTTOM/UTILITY) и фронтовые сокращения (MID/BOT/SUPPORT) сводятся к
/// одному виду, чтобы их можно было сравнивать. Пустая строка — роль неизвестна.
fn normalize_role(role: &str) -> &'static str {
    match role.trim().to_ascii_uppercase().as_str() {
        "TOP" => "TOP",
        "JUNGLE" => "JUNGLE",
        "MID" | "MIDDLE" => "MID",
        "BOT" | "BOTTOM" | "ADC" | "ADCARRY" => "BOT",
        "SUPPORT" | "UTILITY" => "SUPPORT",
        _ => "",
    }
}

/// Считает паттерны игрока по последним матчам. Дорого (≈2 запроса/игру).
/// `count` ограничивается до [`MAX_GAMES`] (число игр, попавших в анализ).
///
/// `filter_champion` / `filter_role` — если заданы, в анализ берутся только игры
/// на этом чемпионе и/или в этой роли. При фильтре сканируется более глубокий
/// пул последних матчей ([`SCAN_CAP`]), т.к. совпадений среди свежих игр мало.
pub fn compute_player_patterns(
    limiter: &SharedLimiter,
    api_key: &str,
    region: &str,
    puuid: &str,
    count: u32,
    filter_champion: Option<u32>,
    filter_role: Option<&str>,
) -> Result<PlayerPatterns, String> {
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("Введите Riot API ключ в Настройках.".into());
    }
    let puuid = puuid.trim().to_string();
    if puuid.is_empty() {
        return Err("Нет PUUID игрока.".into());
    }
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let cfg = RiotConfig {
        api_key,
        region: region.trim().to_string(),
    };

    let n = (count as usize).clamp(1, MAX_GAMES);
    // Канон искомой роли (пустую/неизвестную трактуем как «без фильтра роли»).
    let want_role = filter_role
        .map(normalize_role)
        .filter(|r| !r.is_empty());
    let filtering = filter_champion.is_some() || want_role.is_some();
    // При фильтре сканируем глубже — подходящих игр среди последних мало.
    let fetch_n = if filtering { SCAN_CAP } else { n };
    let ids = fetch_recent_match_ids(&client, &cfg, &limiter.clone(), &puuid, fetch_n)
        .map_err(|e| e.to_user_message())?;

    let mut games: Vec<GamePatterns> = Vec::new();
    for id in &ids {
        // Набрали нужное число подходящих игр — дальше не качаем.
        if games.len() >= n {
            break;
        }
        // Полный матч → роль/чемпион/команда искомого игрока.
        let full = match fetch_match_full(&client, &cfg, limiter, id) {
            Ok(m) => m,
            Err(_) => continue,
        };
        // Только Summoner's Rift (классические очереди). ARAM/арена не дают
        // осмысленных лесных/линейных паттернов.
        if !matches!(full.queue_id, 400 | 420 | 430 | 440 | 700) {
            continue;
        }
        let me = match full.participants.iter().find(|p| p.puuid == puuid) {
            Some(p) => p,
            None => continue,
        };
        // Фильтр по чемпиону текущей игры.
        if let Some(champ) = filter_champion {
            if me.champion_id as u32 != champ {
                continue;
            }
        }
        // Фильтр по роли текущей игры (сравниваем в каноне).
        if let Some(want) = want_role {
            if normalize_role(&me.role) != want {
                continue;
            }
        }
        let role = me.role.clone();
        let champion_id = me.champion_id as u32;
        let is_blue = me.team_id == 100;

        // Таймлайн → события киллов + позиции по кадрам.
        let tl = match fetch_match_timeline_full(&client, &cfg, limiter, id) {
            Ok(t) => t,
            Err(_) => continue,
        };
        // participantId искомого игрока (1..10).
        let my_pid = match tl
            .participant_puuids
            .iter()
            .position(|p| p == &puuid)
        {
            Some(idx) => (idx + 1) as i32,
            None => continue,
        };
        let my_idx = (my_pid - 1) as usize;

        let mut early_invade = false;
        let mut early_kill_part = false;
        let mut deaths = 0u32;
        let mut death_points: Vec<MapPoint> = Vec::new();
        for ev in &tl.events {
            if ev.kind != "kill" {
                continue;
            }
            // Гибель искомого игрока.
            if ev.victim_id == my_pid {
                deaths += 1;
                if ev.x > 0 && ev.y > 0 {
                    death_points.push(to_point(ev.x, ev.y, ev.timestamp_ms / 1000));
                }
            }
            // Участие в килле (киллер или, упрощённо, киллер — ассисты не
            // приходят отдельным полем в GameEvent, поэтому считаем по killerId).
            let participated = ev.killer_id == my_pid;
            if participated {
                if ev.timestamp_ms <= 300_000 {
                    early_kill_part = true;
                }
                // Ранний инвейд: участие в килле в первые ~3 мин во вражеском лесу.
                if ev.timestamp_ms <= EARLY_INVADE_MS
                    && ev.x > 0
                    && ev.y > 0
                    && side_for_position(ev.x, ev.y, is_blue) == "ENEMY"
                {
                    early_invade = true;
                }
            }
        }

        // Позиции игрока по кадрам (минуты), отбрасываем (0,0).
        let mut positions: Vec<MapPoint> = Vec::new();
        for f in &tl.frames {
            if let Some((x, y)) = f.positions.get(my_idx) {
                if *x > 0 && *y > 0 {
                    positions.push(to_point(*x, *y, f.timestamp_ms / 1000));
                }
            }
        }

        // Лесной маршрут: для лесника берём первые ~4 кадра как порядок клира.
        let jungle_route = if role == "JUNGLE" {
            let path: Vec<MapPoint> = positions
                .iter()
                .filter(|p| p.minute <= 4)
                .cloned()
                .collect();
            if let Some(first) = path.first() {
                // Стартовая сторона: считаем по первой известной позиции лесника.
                let fx = (first.x * MAP_MAX) as i32;
                let fy = (first.y * MAP_MAX) as i32;
                let start_side = if side_for_position(fx, fy, is_blue) == "OWN" {
                    "OWN".to_string()
                } else {
                    "ENEMY".to_string()
                };
                Some(JungleRoute { start_side, path })
            } else {
                None
            }
        } else {
            None
        };

        games.push(GamePatterns {
            role,
            champion_id,
            early_invade,
            early_kill_part,
            deaths,
            death_points,
            positions,
            jungle_route,
        });
    }

    Ok(aggregate(&puuid, games))
}

/// Сводит по-игровые данные в `PlayerPatterns` с метками-архетипами.
fn aggregate(puuid: &str, games: Vec<GamePatterns>) -> PlayerPatterns {
    let analyzed = games.len() as u32;
    if games.is_empty() {
        return PlayerPatterns {
            puuid: puuid.to_string(),
            games_analyzed: 0,
            main_role: String::new(),
            main_champion_id: 0,
            main_champion_name: String::new(),
            archetypes: Vec::new(),
            death_points: Vec::new(),
            heatmap: Vec::new(),
            jungle_routes: Vec::new(),
            avg_jungle_route: Vec::new(),
            own_start_fraction: 0.0,
            position_routes: Vec::new(),
        };
    }

    // Основная роль = самая частая.
    let main_role = mode_by(games.iter().map(|g| g.role.clone()));
    // Основной чемпион = самый частый.
    let main_champion_id = mode_by(games.iter().map(|g| g.champion_id)).unwrap_or(0);
    let main_champion_name = if main_champion_id > 0 {
        display_name(main_champion_id)
    } else {
        String::new()
    };

    let total = games.len() as f32;
    let invade_games = games.iter().filter(|g| g.early_invade).count() as f32;
    let early_part_games = games.iter().filter(|g| g.early_kill_part).count() as f32;
    let avg_deaths = games.iter().map(|g| g.deaths).sum::<u32>() as f32 / total;

    let is_jungle = main_role.as_deref() == Some("JUNGLE");

    let mut archetypes: Vec<Archetype> = Vec::new();

    // Инвейдер (особенно показателен для лесника).
    if invade_games > 0.0 {
        let frac = invade_games / total;
        let where_txt = if is_jungle { "в чужом лесу" } else { "на чужой половине" };
        archetypes.push(Archetype {
            label: "Инвейдер".to_string(),
            value: frac,
            explanation: format!(
                "В {}% игр участвует в раннем убийстве {} в первые ~3 мин.",
                (frac * 100.0).round() as i32,
                where_txt
            ),
        });
    }

    // Ранняя агрессия (участие в килле до 5:00).
    if early_part_games > 0.0 {
        let frac = early_part_games / total;
        archetypes.push(Archetype {
            label: "Ранняя агрессия".to_string(),
            value: frac,
            explanation: format!(
                "В {}% игр участвует в убийстве до 5-й минуты.",
                (frac * 100.0).round() as i32
            ),
        });
    }

    // Стиль риска по средним смертям.
    if avg_deaths >= 6.0 {
        archetypes.push(Archetype {
            label: "Рискованный".to_string(),
            value: (avg_deaths / 12.0).clamp(0.0, 1.0),
            explanation: format!("В среднем {:.1} смертей за игру — часто переагрессивит.", avg_deaths),
        });
    } else if avg_deaths <= 3.0 {
        archetypes.push(Archetype {
            label: "Аккуратный".to_string(),
            value: (1.0 - avg_deaths / 6.0).clamp(0.0, 1.0),
            explanation: format!("В среднем всего {:.1} смертей за игру — играет осторожно.", avg_deaths),
        });
    }

    // Сторона старта леса (если лесник): доля игр со стартом в своём лесу.
    if is_jungle {
        let own_start = games
            .iter()
            .filter(|g| g.jungle_route.as_ref().map(|r| r.start_side == "OWN").unwrap_or(false))
            .count() as f32;
        let with_route = games.iter().filter(|g| g.jungle_route.is_some()).count() as f32;
        if with_route > 0.0 {
            let frac = own_start / with_route;
            archetypes.push(Archetype {
                label: "Старт со своего леса".to_string(),
                value: frac,
                explanation: format!(
                    "В {}% игр начинает фарм в своём лесу (остальное — инвейд/чужой старт).",
                    (frac * 100.0).round() as i32
                ),
            });
        }
    }

    // Точки смертей по всем играм.
    let death_points: Vec<MapPoint> = games.iter().flat_map(|g| g.death_points.clone()).collect();

    // Тепловая карта позиций по 5-минутным бинам.
    let heatmap = build_heatmap(&games);

    // ПОЛНЫЕ треки позиций по каждой игре (для линий маршрута перемещений).
    // Позиции уже упорядочены по кадрам (минутам). Берём игры с >= 2 точками.
    let position_routes: Vec<Vec<MapPoint>> = games
        .iter()
        .map(|g| g.positions.clone())
        .filter(|p| p.len() >= 2)
        .collect();

    // Лесные маршруты по играм (сохраняем для совместимости/переиспользования).
    let jungle_routes: Vec<JungleRoute> =
        games.iter().filter_map(|g| g.jungle_route.clone()).collect();

    // Усреднённый «типичный» ранний маршрут + доля старта в своём лесу.
    let avg_jungle_route = average_jungle_route(&jungle_routes);
    let own_start_fraction = if jungle_routes.is_empty() {
        0.0
    } else {
        let own = jungle_routes
            .iter()
            .filter(|r| r.start_side == "OWN")
            .count() as f32;
        own / jungle_routes.len() as f32
    };

    PlayerPatterns {
        puuid: puuid.to_string(),
        games_analyzed: analyzed,
        main_role: main_role.unwrap_or_default(),
        main_champion_id,
        main_champion_name,
        archetypes,
        death_points,
        heatmap,
        jungle_routes,
        avg_jungle_route,
        own_start_fraction,
        position_routes,
    }
}

/// Усредняет ранние лесные маршруты в ОДИН представительный путь: для каждой
/// ранней минуты берём среднюю позицию по всем играм, где она есть, и
/// упорядочиваем по минуте (порядок клира). Возвращает пусто, если маршрутов нет.
fn average_jungle_route(routes: &[JungleRoute]) -> Vec<MapPoint> {
    use std::collections::BTreeMap;
    // minute → (сумма x, сумма y, кол-во).
    let mut acc: BTreeMap<i64, (f32, f32, u32)> = BTreeMap::new();
    for r in routes {
        for p in &r.path {
            let e = acc.entry(p.minute).or_insert((0.0, 0.0, 0));
            e.0 += p.x;
            e.1 += p.y;
            e.2 += 1;
        }
    }
    acc.into_iter()
        .map(|(minute, (sx, sy, n))| {
            let n = n.max(1) as f32;
            MapPoint {
                x: sx / n,
                y: sy / n,
                minute,
                at_seconds: minute * 60,
            }
        })
        .collect()
}

/// Биннинг позиций по окнам `HEATMAP_BIN_MIN` минут. Бины с точками
/// возвращаются по возрастанию минуты.
fn build_heatmap(games: &[GamePatterns]) -> Vec<HeatmapBin> {
    use std::collections::BTreeMap;
    let mut bins: BTreeMap<i64, Vec<MapPoint>> = BTreeMap::new();
    for g in games {
        for p in &g.positions {
            let from = (p.minute / HEATMAP_BIN_MIN) * HEATMAP_BIN_MIN;
            bins.entry(from).or_default().push(p.clone());
        }
    }
    bins.into_iter()
        .map(|(from_minute, points)| HeatmapBin { from_minute, points })
        .collect()
}

/// Самый частый элемент итератора (None для пустого).
fn mode_by<T, I>(iter: I) -> Option<T>
where
    T: std::hash::Hash + Eq + Clone,
    I: Iterator<Item = T>,
{
    use std::collections::HashMap;
    let mut counts: HashMap<T, u32> = HashMap::new();
    for x in iter {
        *counts.entry(x).or_insert(0) += 1;
    }
    counts.into_iter().max_by_key(|(_, c)| *c).map(|(k, _)| k)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_point_normalizes() {
        // Аргумент — секунды; 195с = 3:15 → минута 3.
        let p = to_point(7500, 15000, 195);
        assert!((p.x - 0.5).abs() < 1e-6);
        assert!((p.y - 1.0).abs() < 1e-6);
        assert_eq!(p.minute, 3);
        assert_eq!(p.at_seconds, 195);
    }

    #[test]
    fn to_point_clamps_out_of_range() {
        let p = to_point(-100, 30000, 0);
        assert_eq!(p.x, 0.0);
        assert_eq!(p.y, 1.0);
    }

    // [SAFE-TEST] minute = floor(at_seconds / 60) (целочисленное деление):
    // секунды внутри минуты не округляются вверх.
    #[test]
    fn to_point_minute_is_floor_of_seconds() {
        assert_eq!(to_point(0, 0, 0).minute, 0);
        assert_eq!(to_point(0, 0, 59).minute, 0);
        assert_eq!(to_point(0, 0, 60).minute, 1);
        assert_eq!(to_point(0, 0, 119).minute, 1);
        assert_eq!(to_point(0, 0, 600).minute, 10);
    }

    #[test]
    fn side_blue_own_vs_enemy() {
        // Низ-лево карты для синей команды = свой лес.
        assert_eq!(side_for_position(3000, 3000, true), "OWN");
        // Верх-право карты для синей команды = вражеский лес.
        assert_eq!(side_for_position(12000, 12000, true), "ENEMY");
        // Для красной команды наоборот.
        assert_eq!(side_for_position(3000, 3000, false), "ENEMY");
        assert_eq!(side_for_position(12000, 12000, false), "OWN");
    }

    // [SAFE-TEST] Диагональ x+y < MAP_MAX определяет «синюю» половину (строгое <).
    // Сторона = XOR (near_blue == blue_team), поэтому при фикс. точке смена
    // команды инвертирует OWN/ENEMY.
    #[test]
    fn side_is_xor_of_near_blue_and_team() {
        // Точка в синей половине (x+y < MAP_MAX): для blue — OWN, для red — ENEMY.
        let (x, y) = (3000, 3000);
        assert_eq!(side_for_position(x, y, true), "OWN");
        assert_eq!(side_for_position(x, y, false), "ENEMY");
        // Точка в красной половине: ровно наоборот.
        let (x, y) = (12000, 12000);
        assert_eq!(side_for_position(x, y, true), "ENEMY");
        assert_eq!(side_for_position(x, y, false), "OWN");
    }

    // [SAFE-TEST] Точка РОВНО на диагонали (x + y == MAP_MAX = 15000). Сравнение
    // строгое (<), поэтому near_blue = false → точка относится к красной половине.
    #[test]
    fn side_exactly_on_diagonal_counts_as_red_half() {
        // 7500 + 7500 = 15000 = MAP_MAX, не меньше → near_blue = false.
        // Для синей команды это ENEMY, для красной — OWN.
        assert_eq!(side_for_position(7500, 7500, true), "ENEMY");
        assert_eq!(side_for_position(7500, 7500, false), "OWN");
    }

    // [SAFE-TEST] Канон роли: Riot team_position (MIDDLE/BOTTOM/UTILITY) и
    // фронтовые сокращения (MID/BOT/SUPPORT) сводятся к одному виду; мусор → "".
    #[test]
    fn normalize_role_canonicalizes() {
        assert_eq!(normalize_role("MIDDLE"), "MID");
        assert_eq!(normalize_role("mid"), "MID");
        assert_eq!(normalize_role("BOTTOM"), "BOT");
        assert_eq!(normalize_role("bot"), "BOT");
        assert_eq!(normalize_role("UTILITY"), "SUPPORT");
        assert_eq!(normalize_role(" Support "), "SUPPORT");
        assert_eq!(normalize_role("JUNGLE"), "JUNGLE");
        assert_eq!(normalize_role("TOP"), "TOP");
        assert_eq!(normalize_role(""), "");
        assert_eq!(normalize_role("AFK"), "");
    }

    #[test]
    fn mode_picks_most_frequent() {
        let v = vec!["JUNGLE", "JUNGLE", "MID"];
        assert_eq!(mode_by(v.into_iter()), Some("JUNGLE"));
        let empty: Vec<i32> = vec![];
        assert_eq!(mode_by(empty.into_iter()), None);
    }

    #[test]
    fn aggregate_empty_is_safe() {
        let pp = aggregate("abc", vec![]);
        assert_eq!(pp.games_analyzed, 0);
        assert!(pp.archetypes.is_empty());
    }

    #[test]
    fn aggregate_builds_invader_label() {
        let g = |invade: bool, role: &str| GamePatterns {
            role: role.to_string(),
            champion_id: 64,
            early_invade: invade,
            early_kill_part: invade,
            deaths: 2,
            death_points: vec![],
            positions: vec![to_point(3000, 3000, 1)],
            jungle_route: Some(JungleRoute {
                start_side: "OWN".to_string(),
                path: vec![to_point(3000, 3000, 1)],
            }),
        };
        let games = vec![g(true, "JUNGLE"), g(false, "JUNGLE"), g(true, "JUNGLE")];
        let pp = aggregate("abc", games);
        assert_eq!(pp.games_analyzed, 3);
        assert_eq!(pp.main_role, "JUNGLE");
        // 2 из 3 игр с инвейдом → метка «Инвейдер» присутствует.
        let inv = pp.archetypes.iter().find(|a| a.label == "Инвейдер").unwrap();
        assert!((inv.value - 2.0 / 3.0).abs() < 1e-6);
        // Хитмап непустой, маршруты есть.
        assert!(!pp.heatmap.is_empty());
        assert_eq!(pp.jungle_routes.len(), 3);
        // Все 3 игры стартуют со своего леса → доля 100%.
        assert!((pp.own_start_fraction - 1.0).abs() < 1e-6);
        // Усреднённый маршрут: одна общая минута (0) у всех игр → одна точка.
        assert_eq!(pp.avg_jungle_route.len(), 1);
        assert!((pp.avg_jungle_route[0].x - 0.2).abs() < 1e-6);
    }

    #[test]
    fn average_jungle_route_means_per_minute() {
        // Две игры: разные позиции в одну и ту же минуту усредняются, разные
        // минуты сохраняются отдельными точками, упорядоченными по времени.
        let r1 = JungleRoute {
            start_side: "OWN".to_string(),
            // minute 1 (60с) и minute 2 (120с).
            path: vec![to_point(3000, 3000, 60), to_point(6000, 6000, 120)],
        };
        let r2 = JungleRoute {
            start_side: "ENEMY".to_string(),
            path: vec![to_point(9000, 9000, 60)],
        };
        let avg = average_jungle_route(&[r1, r2]);
        assert_eq!(avg.len(), 2);
        // Минута 1: среднее x = (0.2 + 0.6) / 2 = 0.4.
        assert_eq!(avg[0].minute, 1);
        assert!((avg[0].x - 0.4).abs() < 1e-6);
        // Минута 2: только из r1 → 0.4.
        assert_eq!(avg[1].minute, 2);
        assert!((avg[1].x - 0.4).abs() < 1e-6);
    }

    #[test]
    fn own_start_fraction_mixes_sides() {
        let g = |side: &str| GamePatterns {
            role: "JUNGLE".to_string(),
            champion_id: 64,
            early_invade: false,
            early_kill_part: false,
            deaths: 1,
            death_points: vec![],
            positions: vec![to_point(3000, 3000, 30)],
            jungle_route: Some(JungleRoute {
                start_side: side.to_string(),
                path: vec![to_point(3000, 3000, 30)],
            }),
        };
        // 3 OWN + 1 ENEMY → 75%.
        let pp = aggregate("abc", vec![g("OWN"), g("OWN"), g("OWN"), g("ENEMY")]);
        assert!((pp.own_start_fraction - 0.75).abs() < 1e-6);
    }
}
