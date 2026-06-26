//! Движок рекомендаций пиков v1.
//!
//! Идея: для каждого кандидата в нашей роли берём базовый винрейт (champion_role_agg),
//! корректируем на лайн-матчап (matchup_agg) и синергию с союзниками (synergy_agg),
//! плюс эвристика по составу врага (теги Data Dragon). Всё считается в лог-оддсах,
//! малые выборки сглаживаются по Байесу к базовому винрейту → шум не двигает оценку.
//!
//! champion_vs_comp_agg (контр по точному составу) НЕ используется — мёртв (точный
//! набор из 5 врагов не повторяется). Разложение контра по составу — припарковано.

use crate::db::Database;
use crate::ddragon;

/// Псевдо-веса сглаживания (Байес).
const K_BASE: f64 = 20.0; // база к 50%
const K_MATCHUP: f64 = 30.0; // матчап к базе кандидата
const K_SYNERGY: f64 = 30.0; // синергия к базе кандидата
/// Веса вкладов в лог-оддсах.
const LAMBDA_MATCHUP: f64 = 0.8;
const LAMBDA_SYNERGY: f64 = 0.5;
/// Минимум игр в роли, чтобы чемпион считался кандидатом.
const MIN_ROLE_GAMES: i64 = 20;

pub struct PickInput {
    /// TOP/JUNGLE/MID/BOT/SUPPORT.
    pub my_role: String,
    /// (champion_id, role) вражеских пиков.
    pub enemies: Vec<(i32, String)>,
    /// (champion_id, role) союзных пиков.
    pub allies: Vec<(i32, String)>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickRec {
    pub champion_id: u32,
    pub champion_name: String,
    /// Оценка пика 0..100 (не «вероятность победы»).
    pub score: f32,
    /// Сглаженный базовый винрейт 0..1.
    pub base_win_rate: f32,
    /// Объём данных по кандидату в роли (число игр).
    pub games: i64,
    pub reason: String,
}

// ---------- Чистая математика (тестируемая без БД) ----------

fn logit(p: f64) -> f64 {
    let p = p.clamp(1e-4, 1.0 - 1e-4);
    (p / (1.0 - p)).ln()
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// Байесовское сглаживание винрейта к приору.
fn shrink_wr(wins: i64, games: i64, prior: f64, k: f64) -> f64 {
    (wins as f64 + prior * k) / (games as f64 + k)
}

/// Сборка итоговых лог-оддсов: база + вклад матчапа + средний вклад синергий + эвристика.
/// Каждый вклад — дельта в лог-оддсах относительно базы кандидата.
fn combine_logit(
    base_wr: f64,
    matchup_wr: Option<f64>,
    synergy_wrs: &[f64],
    heuristic: f64,
) -> f64 {
    let bl = logit(base_wr);
    let mut x = bl;
    if let Some(m) = matchup_wr {
        x += LAMBDA_MATCHUP * (logit(m) - bl);
    }
    if !synergy_wrs.is_empty() {
        let avg_delta: f64 =
            synergy_wrs.iter().map(|s| logit(*s) - bl).sum::<f64>() / synergy_wrs.len() as f64;
        x += LAMBDA_SYNERGY * avg_delta;
    }
    x + heuristic
}

// ---------- Эвристика по составу врага (теги Data Dragon) ----------

/// Небольшая поправка в лог-оддсах по составу врага. Положительная — кандидат
/// удачен против такого состава.
fn comp_heuristic(candidate_id: i32, enemies: &[(i32, String)]) -> (f64, Option<String>) {
    let meta = match ddragon::champion_meta(candidate_id as u32) {
        Some(m) => m,
        None => return (0.0, None),
    };
    let is_tank = meta.tags.iter().any(|t| t == "Tank");
    let is_fighter = meta.tags.iter().any(|t| t == "Fighter");

    let mut ad = 0u32;
    let mut ap = 0u32;
    let mut assassins = 0u32;
    for (eid, _) in enemies {
        if let Some(em) = ddragon::champion_meta(*eid as u32) {
            ad += em.attack as u32;
            ap += em.magic as u32;
            if em.tags.iter().any(|t| t == "Assassin") {
                assassins += 1;
            }
        }
    }

    let mut bonus = 0.0;
    let mut reason: Option<String> = None;

    // Танк/боец против преимущественно физического состава.
    if (is_tank || is_fighter) && ad > ap + ap / 3 + 3 {
        bonus += 0.12;
        reason = Some("хорош против физического состава врага".into());
    }
    // Танк против пачки ассасинов.
    if is_tank && assassins >= 2 {
        bonus += 0.12;
        reason = Some("танк против ассасинов врага".into());
    }
    (bonus, reason)
}

// ---------- Главная функция ----------

pub fn recommend(db: &Database, input: &PickInput, limit: usize, patch: &str) -> Result<Vec<PickRec>, String> {
    let role = input.my_role.trim();
    if role.is_empty() {
        return Ok(vec![]);
    }
    // Исключаем уже занятых чемпионов.
    let taken: std::collections::HashSet<i32> = input
        .enemies
        .iter()
        .chain(input.allies.iter())
        .map(|(id, _)| *id)
        .collect();

    let lane_enemy: Option<i32> = input
        .enemies
        .iter()
        .find(|(_, r)| r == role)
        .map(|(id, _)| *id);

    let candidates = db.role_candidates(patch, role, MIN_ROLE_GAMES)?;
    let mut recs: Vec<PickRec> = Vec::new();

    for (cid, games, wins) in candidates {
        if taken.contains(&cid) {
            continue;
        }
        let base = shrink_wr(wins, games, 0.5, K_BASE);

        // Матчап против лайн-оппонента (если известен).
        let mut matchup_wr: Option<f64> = None;
        let mut matchup_games = 0i64;
        if let Some(enemy) = lane_enemy {
            let (mg, mw) = db.matchup_winrate(patch, role, cid, enemy)?;
            if mg > 0 {
                matchup_wr = Some(shrink_wr(mw, mg, base, K_MATCHUP));
                matchup_games = mg;
            }
        }

        // Синергии с союзниками.
        let mut synergy_wrs: Vec<f64> = Vec::new();
        for (aid, arole) in &input.allies {
            let (sg, sw) = db.synergy_winrate(patch, role, cid, arole, *aid)?;
            if sg > 0 {
                synergy_wrs.push(shrink_wr(sw, sg, base, K_SYNERGY));
            }
        }

        let (heur, heur_reason) = comp_heuristic(cid, &input.enemies);
        let final_logit = combine_logit(base, matchup_wr, &synergy_wrs, heur);
        let score = (sigmoid(final_logit) * 100.0) as f32;

        // Причина: самый заметный вклад.
        let reason = build_reason(
            base,
            matchup_wr,
            matchup_games,
            lane_enemy,
            &synergy_wrs,
            heur_reason,
        );

        recs.push(PickRec {
            champion_id: cid as u32,
            champion_name: ddragon::display_name(cid as u32),
            score,
            base_win_rate: base as f32,
            games,
            reason,
        });
    }

    recs.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    recs.truncate(limit);
    Ok(recs)
}

fn build_reason(
    base: f64,
    matchup_wr: Option<f64>,
    matchup_games: i64,
    lane_enemy: Option<i32>,
    synergy_wrs: &[f64],
    heur_reason: Option<String>,
) -> String {
    if let (Some(mwr), Some(enemy)) = (matchup_wr, lane_enemy) {
        if matchup_games >= 20 {
            let pct = (mwr * 100.0).round() as i32;
            let ename = ddragon::display_name(enemy as u32);
            if mwr >= 0.52 {
                return format!("выигрывает лайн против {} ({}%, {} игр)", ename, pct, matchup_games);
            } else if mwr <= 0.48 {
                return format!("сложный матчап против {} ({}%)", ename, pct);
            }
        }
    }
    if let Some(r) = heur_reason {
        return r;
    }
    if !synergy_wrs.is_empty() {
        let avg = synergy_wrs.iter().sum::<f64>() / synergy_wrs.len() as f64;
        if avg >= 0.52 {
            return "хорошая синергия с союзниками".into();
        }
    }
    let pct = (base * 100.0).round() as i32;
    format!("стабильный выбор в роли ({}% винрейт)", pct)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logit_sigmoid_inverse() {
        for p in [0.1, 0.3, 0.5, 0.7, 0.9] {
            let back = sigmoid(logit(p));
            assert!((back - p).abs() < 1e-9, "p={} back={}", p, back);
        }
    }

    #[test]
    fn shrink_no_games_returns_prior() {
        assert!((shrink_wr(0, 0, 0.5, 20.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn shrink_many_games_approaches_raw() {
        // 600 побед из 1000 при большом объёме → близко к 0.6.
        let wr = shrink_wr(600, 1000, 0.5, 20.0);
        assert!((wr - 0.6).abs() < 0.01, "wr={}", wr);
    }

    #[test]
    fn combine_no_signals_equals_base() {
        let x = combine_logit(0.53, None, &[], 0.0);
        assert!((sigmoid(x) - 0.53).abs() < 1e-9);
    }

    #[test]
    fn favorable_matchup_raises_score() {
        let base = 0.50;
        let with = sigmoid(combine_logit(base, Some(0.60), &[], 0.0));
        let without = sigmoid(combine_logit(base, None, &[], 0.0));
        assert!(with > without, "with={} without={}", with, without);
    }

    #[test]
    fn unfavorable_matchup_lowers_score() {
        let base = 0.50;
        let with = sigmoid(combine_logit(base, Some(0.40), &[], 0.0));
        assert!(with < base, "with={}", with);
    }

    #[test]
    fn positive_heuristic_raises_score() {
        let base = 0.50;
        let with = sigmoid(combine_logit(base, None, &[], 0.12));
        assert!(with > base);
    }

    #[test]
    fn synergy_averages_deltas() {
        let base = 0.50;
        // Две синергии выше базы → итог выше базы.
        let with = sigmoid(combine_logit(base, None, &[0.55, 0.57], 0.0));
        assert!(with > base, "with={}", with);
    }
}
