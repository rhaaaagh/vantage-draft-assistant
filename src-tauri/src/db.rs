use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Патч "16.12" → (16, 12) для численного сравнения. Сравнивать строкой нельзя:
/// "16.2" > "16.12" как текст, но патч 12 новее патча 2.
pub fn parse_patch(s: &str) -> (i64, i64) {
    let mut it = s.split('.');
    let major = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
    let minor = it.next().and_then(|x| x.trim().parse().ok()).unwrap_or(0);
    (major, minor)
}

/// Уровень ранга, который мы поддерживаем для аналитики.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RankBucket {
    EmeraldPlus,
}

/// Агрегированная статистика чемпиона против композиции врага.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChampionVsCompAgg {
    pub champion_id: i32,
    /// Хэш состава вражеской команды (например, отсортированный список champion_id).
    pub enemy_comp_hash: String,
    pub patch_bucket: String,
    pub rank_bucket: RankBucket,
    pub games: i32,
    pub wins: i32,
}

/// Агрегированная статистика билдов предметов.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildAgg {
    pub champion_id: i32,
    pub enemy_comp_hash: Option<String>,
    pub item_build_hash: String,
    pub games: i32,
    pub wins: i32,
}

/// Агрегат по чемпиону и роли для тир-листа (Emerald+).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChampionRoleStat {
    pub champion_id: i32,
    pub role: String,
    pub patch: String,
    pub games: i32,
    pub wins: i32,
}

/// Один игрок матча для записи краулером.
pub struct CrawlPlayer {
    pub puuid: String,
    pub champion_id: i32,
    pub team_id: i32,
    /// TOP/JUNGLE/MID/BOT/SUPPORT или "" если роль неизвестна.
    pub role: String,
    pub win: bool,
    pub kills: i32,
    pub deaths: i32,
    pub assists: i32,
    pub cs: i32,
    pub items: [i32; 7],
    /// Первый собранный завершённый предмет (0 если неизвестно).
    pub first_item: i32,
    /// Первые купленные ботинки (0 если нет).
    pub boots: i32,
    /// Завершённые предметы в порядке покупки, БЕЗ ботинок (для пути сборки).
    pub ordered_items: Vec<i32>,
    /// Кейстоун (первая руна основного древа), 0 если неизвестно.
    pub keystone_id: i32,
    /// ID основного древа рун, 0 если неизвестно.
    pub primary_style_id: i32,
    /// ID вторичного древа рун, 0 если неизвестно.
    pub sub_style_id: i32,
}

/// Агрегат рун чемпиона: один ряд = руна определённого вида с играми и победами.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuneRow {
    /// Вид руны: "keystone" | "primary" | "secondary".
    pub kind: String,
    pub rune_id: i32,
    pub games: i64,
    pub wins: i64,
}

/// Матч для записи краулером (сырьё + источник всех агрегатов).
pub struct CrawlMatch {
    pub match_id: String,
    pub patch: String,
    pub queue_id: i32,
    pub duration: i64,
    pub players: Vec<CrawlPlayer>,
    /// (champion_id, team_id) забаненных.
    pub bans: Vec<(i32, i32)>,
}

/// Агрегат строки мета тир-листа из champion_role_agg.
#[derive(Debug, Clone)]
pub struct TierRow {
    pub champion_id: i32,
    pub role: String,
    pub patch: String,
    pub games: i64,
    pub wins: i64,
}

/// Простая обёртка над SQLite.
pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &std::path::Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<(), String> {
        // WAL — лучше для частых записей краулера.
        let _ = self.conn.execute_batch("PRAGMA journal_mode=WAL;");
        self.conn
            .execute_batch(
                r#"
CREATE TABLE IF NOT EXISTS matches (
  match_id TEXT PRIMARY KEY,
  patch TEXT NOT NULL,
  queue_id INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS participants (
  match_id TEXT NOT NULL,
  puuid TEXT NOT NULL,
  team_id INTEGER NOT NULL,
  champion_id INTEGER NOT NULL,
  role TEXT,
  lane TEXT,
  win INTEGER NOT NULL,
  rank_tier TEXT NOT NULL,
  item0 INTEGER,
  item1 INTEGER,
  item2 INTEGER,
  item3 INTEGER,
  item4 INTEGER,
  item5 INTEGER,
  item6 INTEGER,
  PRIMARY KEY (match_id, puuid)
);

CREATE TABLE IF NOT EXISTS champion_vs_comp_agg (
  champion_id INTEGER NOT NULL,
  enemy_comp_hash TEXT NOT NULL,
  patch_bucket TEXT NOT NULL,
  rank_bucket TEXT NOT NULL,
  games INTEGER NOT NULL,
  wins INTEGER NOT NULL,
  PRIMARY KEY (champion_id, enemy_comp_hash, patch_bucket, rank_bucket)
);

CREATE TABLE IF NOT EXISTS build_agg (
  champion_id INTEGER NOT NULL,
  enemy_comp_hash TEXT,
  item_build_hash TEXT NOT NULL,
  games INTEGER NOT NULL,
  wins INTEGER NOT NULL,
  PRIMARY KEY (champion_id, enemy_comp_hash, item_build_hash)
);

-- Этап 2: попарные матчапы чемпион-против-чемпиона в одной роли.
CREATE TABLE IF NOT EXISTS matchup_agg (
  patch TEXT NOT NULL,
  role TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  enemy_champion_id INTEGER NOT NULL,
  games INTEGER NOT NULL DEFAULT 0,
  wins INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (patch, role, champion_id, enemy_champion_id)
);

-- Базовый винрейт чемпиона в роли (для shrinkage и тир-листа).
CREATE TABLE IF NOT EXISTS champion_role_agg (
  patch TEXT NOT NULL,
  role TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  games INTEGER NOT NULL DEFAULT 0,
  wins INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (patch, role, champion_id)
);

-- Синергия: чемпион в роли + союзник в другой роли.
CREATE TABLE IF NOT EXISTS synergy_agg (
  patch TEXT NOT NULL,
  role TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  ally_role TEXT NOT NULL,
  ally_champion_id INTEGER NOT NULL,
  games INTEGER NOT NULL DEFAULT 0,
  wins INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (patch, role, champion_id, ally_role, ally_champion_id)
);

-- Баны чемпиона по патчу (для бан-рейта).
CREATE TABLE IF NOT EXISTS ban_agg (
  patch TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  games INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (patch, champion_id)
);

-- Сколько всего матчей обработано по патчу (знаменатель пик/бан-рейта).
CREATE TABLE IF NOT EXISTS patch_totals (
  patch TEXT PRIMARY KEY,
  matches INTEGER NOT NULL DEFAULT 0
);

-- Статистика предметов: is_first=1 — первый собранный, 0 — встречается в финальном билде.
CREATE TABLE IF NOT EXISTS item_agg (
  patch TEXT NOT NULL,
  role TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  item_id INTEGER NOT NULL,
  is_first INTEGER NOT NULL,
  games INTEGER NOT NULL DEFAULT 0,
  wins INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (patch, role, champion_id, item_id, is_first)
);

-- Порядок предметов: slot 0 = ботинки, 1..6 = завершённые предметы по порядку покупки.
CREATE TABLE IF NOT EXISTS item_order_agg (
  patch TEXT NOT NULL,
  role TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  slot INTEGER NOT NULL,
  item_id INTEGER NOT NULL,
  games INTEGER NOT NULL DEFAULT 0,
  wins INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (patch, role, champion_id, slot, item_id)
);

-- Статистика рун: kind = 'keystone' | 'primary' | 'secondary'. rune_id — perkId/styleId.
CREATE TABLE IF NOT EXISTS rune_agg (
  patch TEXT NOT NULL,
  role TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  kind TEXT NOT NULL,
  rune_id INTEGER NOT NULL,
  games INTEGER NOT NULL DEFAULT 0,
  wins INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (patch, role, champion_id, kind, rune_id)
);

-- Сырые матчи (для пересчёта любых метрик без перекраулинга).
CREATE TABLE IF NOT EXISTS cmatch (
  match_id TEXT PRIMARY KEY,
  patch TEXT NOT NULL,
  queue_id INTEGER NOT NULL,
  duration INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS cpart (
  match_id TEXT NOT NULL,
  puuid TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  team_id INTEGER NOT NULL,
  role TEXT NOT NULL,
  win INTEGER NOT NULL,
  kills INTEGER NOT NULL,
  deaths INTEGER NOT NULL,
  assists INTEGER NOT NULL,
  cs INTEGER NOT NULL,
  item0 INTEGER, item1 INTEGER, item2 INTEGER, item3 INTEGER,
  item4 INTEGER, item5 INTEGER, item6 INTEGER,
  first_item INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (match_id, puuid)
);
CREATE TABLE IF NOT EXISTS cban (
  match_id TEXT NOT NULL,
  champion_id INTEGER NOT NULL,
  team_id INTEGER NOT NULL
);
-- кэш агрегатов «паттернов игрока» (player-pattern engine).
-- Тяжёлый расчёт по таймлайнам N последних игр кэшируется по puuid, чтобы не
-- перекачивать таймлайны при каждом открытии карточки врага в Скауте.
-- payload — сериализованный JSON результата (архетипы + точки смертей + хитмап).
CREATE TABLE IF NOT EXISTS player_pattern_cache (
  puuid TEXT PRIMARY KEY,
  games INTEGER NOT NULL,
  computed_at INTEGER NOT NULL,
  payload TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_synergy_lookup ON synergy_agg(role, champion_id);
CREATE INDEX IF NOT EXISTS idx_item_lookup ON item_agg(role, champion_id, is_first);
CREATE INDEX IF NOT EXISTS idx_item_order_lookup ON item_order_agg(role, champion_id, slot);
CREATE INDEX IF NOT EXISTS idx_rune_lookup ON rune_agg(role, champion_id, kind);
CREATE INDEX IF NOT EXISTS idx_cpart_champ ON cpart(champion_id, role);

-- Состояние краулера (резюмируемость).
CREATE TABLE IF NOT EXISTS crawl_puuids (
  puuid TEXT PRIMARY KEY,
  tier TEXT,
  status TEXT NOT NULL DEFAULT 'pending'
);
CREATE TABLE IF NOT EXISTS crawl_matches (
  match_id TEXT PRIMARY KEY,
  status TEXT NOT NULL DEFAULT 'pending'
);
CREATE INDEX IF NOT EXISTS idx_crawl_matches_status ON crawl_matches(status);
CREATE INDEX IF NOT EXISTS idx_crawl_puuids_status ON crawl_puuids(status);
CREATE INDEX IF NOT EXISTS idx_matchup_lookup ON matchup_agg(role, champion_id, enemy_champion_id);
CREATE INDEX IF NOT EXISTS idx_role_agg_lookup ON champion_role_agg(role, champion_id);
"#,
            )
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn insert_match(&self, match_id: &str, patch: &str, queue_id: i32) -> Result<(), String> {
        self.conn
            .execute(
                r#"
INSERT INTO matches (match_id, patch, queue_id)
VALUES (?1, ?2, ?3)
ON CONFLICT(match_id) DO UPDATE SET
  patch = excluded.patch,
  queue_id = excluded.queue_id;
"#,
                params![match_id, patch, queue_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_participant(
        &self,
        match_id: &str,
        puuid: &str,
        team_id: i32,
        champion_id: i32,
        role: Option<&str>,
        lane: Option<&str>,
        win: bool,
        rank_tier: &str,
        items: [i32; 7],
    ) -> Result<(), String> {
        self.conn
            .execute(
                r#"
INSERT INTO participants (
  match_id, puuid, team_id, champion_id, role, lane, win, rank_tier,
  item0, item1, item2, item3, item4, item5, item6
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
          ?9, ?10, ?11, ?12, ?13, ?14, ?15)
ON CONFLICT(match_id, puuid) DO UPDATE SET
  team_id = excluded.team_id,
  champion_id = excluded.champion_id,
  role = excluded.role,
  lane = excluded.lane,
  win = excluded.win,
  rank_tier = excluded.rank_tier,
  item0 = excluded.item0,
  item1 = excluded.item1,
  item2 = excluded.item2,
  item3 = excluded.item3,
  item4 = excluded.item4,
  item5 = excluded.item5,
  item6 = excluded.item6;
"#,
                params![
                    match_id,
                    puuid,
                    team_id,
                    champion_id,
                    role,
                    lane,
                    if win { 1 } else { 0 },
                    rank_tier,
                    items[0],
                    items[1],
                    items[2],
                    items[3],
                    items[4],
                    items[5],
                    items[6],
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn upsert_champion_vs_comp(&self, agg: &ChampionVsCompAgg) -> Result<(), String> {
        self.conn
            .execute(
                r#"
INSERT INTO champion_vs_comp_agg (
  champion_id, enemy_comp_hash, patch_bucket, rank_bucket, games, wins
) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
ON CONFLICT(champion_id, enemy_comp_hash, patch_bucket, rank_bucket)
DO UPDATE SET
  games = games + excluded.games,
  wins = wins + excluded.wins;
"#,
                params![
                    agg.champion_id,
                    agg.enemy_comp_hash,
                    agg.patch_bucket,
                    "EMERALD_PLUS",
                    agg.games,
                    agg.wins
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn upsert_build(&self, agg: &BuildAgg) -> Result<(), String> {
        self.conn
            .execute(
                r#"
INSERT INTO build_agg (
  champion_id, enemy_comp_hash, item_build_hash, games, wins
) VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(champion_id, enemy_comp_hash, item_build_hash)
DO UPDATE SET
  games = games + excluded.games,
  wins = wins + excluded.wins;
"#,
                params![
                    agg.champion_id,
                    agg.enemy_comp_hash,
                    agg.item_build_hash,
                    agg.games,
                    agg.wins
                ],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn rank_bucket_str(bucket: RankBucket) -> &'static str {
        match bucket {
            RankBucket::EmeraldPlus => "EMERALD_PLUS",
        }
    }

    pub fn top_champions_vs_comp(
        &self,
        enemy_comp_hash: &str,
        patch_bucket: &str,
        rank_bucket: RankBucket,
        min_games: i32,
        limit: i32,
    ) -> Result<Vec<ChampionVsCompAgg>, String> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
SELECT champion_id, enemy_comp_hash, patch_bucket, rank_bucket, games, wins
FROM champion_vs_comp_agg
WHERE enemy_comp_hash = ?1
  AND patch_bucket = ?2
  AND rank_bucket = ?3
  AND games >= ?4
ORDER BY (CAST(wins AS REAL) / CAST(games AS REAL)) DESC, games DESC
LIMIT ?5;
"#,
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(
                params![
                    enemy_comp_hash,
                    patch_bucket,
                    Self::rank_bucket_str(rank_bucket),
                    min_games,
                    limit
                ],
                |row| {
                    Ok(ChampionVsCompAgg {
                        champion_id: row.get(0)?,
                        enemy_comp_hash: row.get(1)?,
                        patch_bucket: row.get(2)?,
                        rank_bucket: RankBucket::EmeraldPlus,
                        games: row.get(4)?,
                        wins: row.get(5)?,
                    })
                },
            )
            .map_err(|e| e.to_string())?;

        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn bottom_champions_vs_comp(
        &self,
        enemy_comp_hash: &str,
        patch_bucket: &str,
        rank_bucket: RankBucket,
        min_games: i32,
        limit: i32,
    ) -> Result<Vec<ChampionVsCompAgg>, String> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
SELECT champion_id, enemy_comp_hash, patch_bucket, rank_bucket, games, wins
FROM champion_vs_comp_agg
WHERE enemy_comp_hash = ?1
  AND patch_bucket = ?2
  AND rank_bucket = ?3
  AND games >= ?4
ORDER BY (CAST(wins AS REAL) / CAST(games AS REAL)) ASC, games DESC
LIMIT ?5;
"#,
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(
                params![
                    enemy_comp_hash,
                    patch_bucket,
                    Self::rank_bucket_str(rank_bucket),
                    min_games,
                    limit
                ],
                |row| {
                    Ok(ChampionVsCompAgg {
                        champion_id: row.get(0)?,
                        enemy_comp_hash: row.get(1)?,
                        patch_bucket: row.get(2)?,
                        rank_bucket: RankBucket::EmeraldPlus,
                        games: row.get(4)?,
                        wins: row.get(5)?,
                    })
                },
            )
            .map_err(|e| e.to_string())?;

        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn top_builds(
        &self,
        champion_id: i32,
        enemy_comp_hash: Option<&str>,
        min_games: i32,
        limit: i32,
    ) -> Result<Vec<BuildAgg>, String> {
        if let Some(hash) = enemy_comp_hash {
            let sql = r#"
SELECT champion_id, enemy_comp_hash, item_build_hash, games, wins
FROM build_agg
WHERE champion_id = ?1
  AND enemy_comp_hash = ?2
  AND games >= ?3
ORDER BY (CAST(wins AS REAL) / CAST(games AS REAL)) DESC, games DESC
LIMIT ?4;
"#;
            let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(
                    params![champion_id, hash, min_games, limit],
                    |row| {
                        Ok(BuildAgg {
                            champion_id: row.get(0)?,
                            enemy_comp_hash: row.get(1)?,
                            item_build_hash: row.get(2)?,
                            games: row.get(3)?,
                            wins: row.get(4)?,
                        })
                    },
                )
                .map_err(|e| e.to_string())?;

            Ok(rows.filter_map(Result::ok).collect())
        } else {
            let sql = r#"
SELECT champion_id, enemy_comp_hash, item_build_hash, games, wins
FROM build_agg
WHERE champion_id = ?1
  AND games >= ?2
ORDER BY (CAST(wins AS REAL) / CAST(games AS REAL)) DESC, games DESC
LIMIT ?3;
"#;
            let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(
                    params![champion_id, min_games, limit],
                    |row| {
                        Ok(BuildAgg {
                            champion_id: row.get(0)?,
                            enemy_comp_hash: row.get(1)?,
                            item_build_hash: row.get(2)?,
                            games: row.get(3)?,
                            wins: row.get(4)?,
                        })
                    },
                )
                .map_err(|e| e.to_string())?;

            Ok(rows.filter_map(Result::ok).collect())
        }
    }

    /// Роли Riot: TOP, JUNGLE, MID, BOTTOM, UTILITY (individual_position).
    /// 'EMERALD_PLUS' — метка, которую пишет sync_matches для аккаунта пользователя;
    /// остальные — на случай, если краулер (Этап 2) запишет реальный тир игрока.
    const EMERALD_PLUS_TIERS: &str =
        "'EMERALD_PLUS','EMERALD','DIAMOND','MASTER','GRANDMASTER','CHALLENGER'";

    /// Список патчей, по которым есть данные (для фильтра тир-листа).
    pub fn get_tier_list_patches(&self) -> Result<Vec<String>, String> {
        let sql = format!(
            r#"
            SELECT DISTINCT m.patch
            FROM participants p
            JOIN matches m ON p.match_id = m.match_id
            WHERE p.rank_tier IN ({})
            ORDER BY m.patch DESC
            LIMIT 20
            "#,
            Self::EMERALD_PLUS_TIERS
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get(0)).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Список ролей, по которым есть данные (для фильтра тир-листа).
    pub fn get_tier_list_roles(&self) -> Result<Vec<String>, String> {
        let sql = format!(
            r#"
            SELECT DISTINCT p.role
            FROM participants p
            WHERE p.rank_tier IN ({}) AND p.role IS NOT NULL AND p.role != ''
            ORDER BY p.role
            "#,
            Self::EMERALD_PLUS_TIERS
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get(0)).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Тир-лист: чемпионы по роли и патчу с играми и винрейтом (Emerald+).
    /// patch_filter/role_filter: пустая строка = без фильтра.
    pub fn get_champion_role_stats(
        &self,
        patch_filter: &str,
        role_filter: &str,
        min_games: i32,
        limit: i32,
    ) -> Result<Vec<ChampionRoleStat>, String> {
        let sql = format!(
            r#"
            SELECT p.champion_id, COALESCE(p.role, '') AS role, m.patch,
                   COUNT(*) AS games, SUM(p.win) AS wins
            FROM participants p
            JOIN matches m ON p.match_id = m.match_id
            WHERE p.rank_tier IN ({})
              AND (?1 = '' OR m.patch = ?1)
              AND (?2 = '' OR p.role = ?2)
            GROUP BY p.champion_id, p.role, m.patch
            HAVING COUNT(*) >= ?3
            ORDER BY (CAST(SUM(p.win) AS REAL) / COUNT(*)) DESC, COUNT(*) DESC
            LIMIT ?4
            "#,
            Self::EMERALD_PLUS_TIERS
        );
        let mut stmt = self.conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(
                params![patch_filter, role_filter, min_games, limit],
                |row| {
                    Ok(ChampionRoleStat {
                        champion_id: row.get(0)?,
                        role: row.get(1)?,
                        patch: row.get(2)?,
                        games: row.get(3)?,
                        wins: row.get(4)?,
                    })
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Возвращает последний доступный patch_bucket для заданного enemy_comp_hash и ранга.
    pub fn latest_patch_for_enemy_comp(
        &self,
        enemy_comp_hash: &str,
        rank_bucket: RankBucket,
    ) -> Result<Option<String>, String> {
        let mut stmt = self
            .conn
            .prepare(
                r#"
SELECT patch_bucket
FROM champion_vs_comp_agg
WHERE enemy_comp_hash = ?1
  AND rank_bucket = ?2
ORDER BY patch_bucket DESC
LIMIT 1;
"#,
            )
            .map_err(|e| e.to_string())?;

        let mut rows = stmt
            .query(params![enemy_comp_hash, Self::rank_bucket_str(rank_bucket)])
            .map_err(|e| e.to_string())?;

        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let patch: String = row.get::<_, String>(0).map_err(|e| e.to_string())?;
            Ok(Some(patch))
        } else {
            Ok(None)
        }
    }

    // ---------- Этап 2: агрегаты матчей ----------

    /// Полная запись матча: сырьё (cmatch/cpart/cban) + все агрегаты — в одной транзакции.
    pub fn record_match(&self, m: &CrawlMatch) -> Result<(), String> {
        let tx = self.conn.unchecked_transaction().map_err(|e| e.to_string())?;

        // Идемпотентность: если матч уже записан, второй раз агрегаты не трогаем.
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO cmatch (match_id, patch, queue_id, duration) VALUES (?1,?2,?3,?4);",
            params![m.match_id, m.patch, m.queue_id, m.duration],
        ).map_err(|e| e.to_string())?;
        if inserted == 0 {
            tx.commit().map_err(|e| e.to_string())?;
            return Ok(());
        }
        tx.execute(
            "INSERT OR IGNORE INTO patch_totals (patch, matches) VALUES (?1, 0);",
            params![m.patch],
        ).map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE patch_totals SET matches = matches + 1 WHERE patch = ?1;",
            params![m.patch],
        ).map_err(|e| e.to_string())?;

        // Баны
        for (champ, team) in &m.bans {
            if *champ <= 0 { continue; }
            tx.execute(
                "INSERT OR IGNORE INTO cban (match_id, champion_id, team_id) VALUES (?1,?2,?3);",
                params![m.match_id, champ, team],
            ).map_err(|e| e.to_string())?;
            tx.execute(
                "INSERT INTO ban_agg (patch, champion_id, games) VALUES (?1,?2,1)
                 ON CONFLICT(patch, champion_id) DO UPDATE SET games = games + 1;",
                params![m.patch, champ],
            ).map_err(|e| e.to_string())?;
        }

        for p in &m.players {
            let w = if p.win { 1 } else { 0 };
            tx.execute(
                "INSERT OR IGNORE INTO cpart
                 (match_id, puuid, champion_id, team_id, role, win, kills, deaths, assists, cs,
                  item0,item1,item2,item3,item4,item5,item6, first_item)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18);",
                params![m.match_id, p.puuid, p.champion_id, p.team_id, p.role, w,
                    p.kills, p.deaths, p.assists, p.cs,
                    p.items[0],p.items[1],p.items[2],p.items[3],p.items[4],p.items[5],p.items[6], p.first_item],
            ).map_err(|e| e.to_string())?;

            if p.role.is_empty() { continue; }
            // Базовый винрейт в роли
            tx.execute(
                "INSERT INTO champion_role_agg (patch, role, champion_id, games, wins) VALUES (?1,?2,?3,1,?4)
                 ON CONFLICT(patch, role, champion_id) DO UPDATE SET games=games+1, wins=wins+?4;",
                params![m.patch, p.role, p.champion_id, w],
            ).map_err(|e| e.to_string())?;
            // Предметы финального билда
            for it in p.items.iter().filter(|x| **x > 0) {
                tx.execute(
                    "INSERT INTO item_agg (patch, role, champion_id, item_id, is_first, games, wins) VALUES (?1,?2,?3,?4,0,1,?5)
                     ON CONFLICT(patch, role, champion_id, item_id, is_first) DO UPDATE SET games=games+1, wins=wins+?5;",
                    params![m.patch, p.role, p.champion_id, it, w],
                ).map_err(|e| e.to_string())?;
            }
            // Первый собранный предмет
            if p.first_item > 0 {
                tx.execute(
                    "INSERT INTO item_agg (patch, role, champion_id, item_id, is_first, games, wins) VALUES (?1,?2,?3,?4,1,1,?5)
                     ON CONFLICT(patch, role, champion_id, item_id, is_first) DO UPDATE SET games=games+1, wins=wins+?5;",
                    params![m.patch, p.role, p.champion_id, p.first_item, w],
                ).map_err(|e| e.to_string())?;
            }
            // Ботинки (slot 0)
            if p.boots > 0 {
                tx.execute(
                    "INSERT INTO item_order_agg (patch, role, champion_id, slot, item_id, games, wins) VALUES (?1,?2,?3,0,?4,1,?5)
                     ON CONFLICT(patch, role, champion_id, slot, item_id) DO UPDATE SET games=games+1, wins=wins+?5;",
                    params![m.patch, p.role, p.champion_id, p.boots, w],
                ).map_err(|e| e.to_string())?;
            }
            // Завершённые предметы по порядку (slot 1..6, без ботинок)
            for (idx, it) in p.ordered_items.iter().take(6).enumerate() {
                if *it <= 0 { continue; }
                let slot = (idx + 1) as i32;
                tx.execute(
                    "INSERT INTO item_order_agg (patch, role, champion_id, slot, item_id, games, wins) VALUES (?1,?2,?3,?4,?5,1,?6)
                     ON CONFLICT(patch, role, champion_id, slot, item_id) DO UPDATE SET games=games+1, wins=wins+?6;",
                    params![m.patch, p.role, p.champion_id, slot, it, w],
                ).map_err(|e| e.to_string())?;
            }
            // Руны: кейстоун + основное/вторичное древо (пропускаем нулевые id).
            for (kind, rune_id) in [
                ("keystone", p.keystone_id),
                ("primary", p.primary_style_id),
                ("secondary", p.sub_style_id),
            ] {
                if rune_id <= 0 { continue; }
                tx.execute(
                    "INSERT INTO rune_agg (patch, role, champion_id, kind, rune_id, games, wins) VALUES (?1,?2,?3,?4,?5,1,?6)
                     ON CONFLICT(patch, role, champion_id, kind, rune_id) DO UPDATE SET games=games+1, wins=wins+?6;",
                    params![m.patch, p.role, p.champion_id, kind, rune_id, w],
                ).map_err(|e| e.to_string())?;
            }
        }

        // Матчапы (враг той же роли) и синергия (союзник другой роли)
        for a in &m.players {
            if a.role.is_empty() { continue; }
            let wa = if a.win { 1 } else { 0 };
            for b in &m.players {
                if b.role.is_empty() || a.puuid == b.puuid { continue; }
                if a.team_id != b.team_id && a.role == b.role {
                    tx.execute(
                        "INSERT INTO matchup_agg (patch, role, champion_id, enemy_champion_id, games, wins) VALUES (?1,?2,?3,?4,1,?5)
                         ON CONFLICT(patch, role, champion_id, enemy_champion_id) DO UPDATE SET games=games+1, wins=wins+?5;",
                        params![m.patch, a.role, a.champion_id, b.champion_id, wa],
                    ).map_err(|e| e.to_string())?;
                } else if a.team_id == b.team_id && a.role != b.role {
                    tx.execute(
                        "INSERT INTO synergy_agg (patch, role, champion_id, ally_role, ally_champion_id, games, wins) VALUES (?1,?2,?3,?4,?5,1,?6)
                         ON CONFLICT(patch, role, champion_id, ally_role, ally_champion_id) DO UPDATE SET games=games+1, wins=wins+?6;",
                        params![m.patch, a.role, a.champion_id, b.role, b.champion_id, wa],
                    ).map_err(|e| e.to_string())?;
                }
            }
        }

        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---------- Этап 2: состояние краулера ----------

    pub fn add_crawl_puuid(&self, puuid: &str, tier: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO crawl_puuids (puuid, tier, status) VALUES (?1, ?2, 'pending');",
                params![puuid, tier],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn next_pending_puuid(&self) -> Result<Option<(String, String)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT puuid, COALESCE(tier,'') FROM crawl_puuids WHERE status='pending' LIMIT 1;")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some((
                row.get::<_, String>(0).map_err(|e| e.to_string())?,
                row.get::<_, String>(1).map_err(|e| e.to_string())?,
            )))
        } else {
            Ok(None)
        }
    }

    pub fn mark_puuid_done(&self, puuid: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE crawl_puuids SET status='done' WHERE puuid=?1;",
                params![puuid],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn add_crawl_match(&self, match_id: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT OR IGNORE INTO crawl_matches (match_id, status) VALUES (?1, 'pending');",
                params![match_id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn next_pending_match(&self) -> Result<Option<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT match_id FROM crawl_matches WHERE status='pending' LIMIT 1;")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(Some(row.get::<_, String>(0).map_err(|e| e.to_string())?))
        } else {
            Ok(None)
        }
    }

    pub fn mark_match(&self, match_id: &str, status: &str) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE crawl_matches SET status=?2 WHERE match_id=?1;",
                params![match_id, status],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    fn count_where(&self, sql: &str) -> Result<i64, String> {
        let mut stmt = self.conn.prepare(sql).map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok(row.get::<_, i64>(0).map_err(|e| e.to_string())?)
        } else {
            Ok(0)
        }
    }

    /// (puuids_total, puuids_done, matches_total, matches_done)
    pub fn crawl_counts(&self) -> Result<(i64, i64, i64, i64), String> {
        Ok((
            self.count_where("SELECT COUNT(*) FROM crawl_puuids;")?,
            self.count_where("SELECT COUNT(*) FROM crawl_puuids WHERE status='done';")?,
            self.count_where("SELECT COUNT(*) FROM crawl_matches;")?,
            self.count_where("SELECT COUNT(*) FROM crawl_matches WHERE status='done';")?,
        ))
    }

    pub fn has_seeds(&self) -> Result<bool, String> {
        Ok(self.count_where("SELECT COUNT(*) FROM crawl_puuids;")? > 0)
    }

    /// Полный сброс собранной статистики и очереди краулера (для пересбора с нуля).
    pub fn reset_crawl_data(&self) -> Result<(), String> {
        for sql in [
            "DELETE FROM matchup_agg;",
            "DELETE FROM champion_role_agg;",
            "DELETE FROM synergy_agg;",
            "DELETE FROM ban_agg;",
            "DELETE FROM item_agg;",
            "DELETE FROM rune_agg;",
            "DELETE FROM patch_totals;",
            "DELETE FROM cmatch;",
            "DELETE FROM cpart;",
            "DELETE FROM cban;",
            "DELETE FROM crawl_matches;",
            "DELETE FROM crawl_puuids;",
        ] {
            self.conn.execute(sql, []).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Самый высокий патч в базе, который численно ниже current (или None).
    pub fn previous_patch(&self, current: &str) -> Result<Option<String>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT patch FROM patch_totals;")
            .map_err(|e| e.to_string())?;
        let cur = parse_patch(current);
        let prev = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .filter(|p| parse_patch(p) < cur)
            .max_by_key(|p| parse_patch(p));
        Ok(prev)
    }

    /// Оставляет данные только текущего и одного предыдущего патча, остальное удаляет.
    /// «Предыдущий» — самый высокий патч в базе, который численно ниже текущего
    /// (сравнение по (major, minor), а не по строке: 16.2 < 16.12). current оставляем
    /// всегда, даже если его ещё нет в данных (только что вышел).
    pub fn purge_keep_current_and_prev(&self, current: &str) -> Result<(), String> {
        // Все патчи, присутствующие в данных.
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT patch FROM patch_totals;")
            .map_err(|e| e.to_string())?;
        let patches: Vec<String> = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect();
        drop(stmt);

        let cur = parse_patch(current);
        // Предыдущий = максимальный патч строго ниже текущего.
        let prev: Option<String> = patches
            .iter()
            .filter(|p| parse_patch(p) < cur)
            .max_by_key(|p| parse_patch(p))
            .cloned();

        // Множество патчей, которые оставляем.
        let mut keep: Vec<String> = vec![current.to_string()];
        if let Some(p) = prev {
            keep.push(p);
        }

        // Удаляем всё, что не в keep. Список keep маленький (1-2), строим IN вручную
        // через параметры.
        let placeholders = keep.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let keep_params: Vec<&dyn rusqlite::ToSql> =
            keep.iter().map(|s| s as &dyn rusqlite::ToSql).collect();
        for table in [
            "matchup_agg",
            "champion_role_agg",
            "synergy_agg",
            "ban_agg",
            "item_agg",
            "item_order_agg",
            "rune_agg",
            "patch_totals",
            "cmatch",
        ] {
            let sql = format!("DELETE FROM {} WHERE patch NOT IN ({});", table, placeholders);
            self.conn
                .execute(&sql, keep_params.as_slice())
                .map_err(|e| e.to_string())?;
        }
        // cpart/cban чистим по матчам, которых уже нет в cmatch.
        self.conn.execute(
            "DELETE FROM cpart WHERE match_id NOT IN (SELECT match_id FROM cmatch);", [],
        ).map_err(|e| e.to_string())?;
        self.conn.execute(
            "DELETE FROM cban WHERE match_id NOT IN (SELECT match_id FROM cmatch);", [],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    // ---------- Этап 2: запросы для рекомендаций ----------

    /// Винрейт матчапа champion vs enemy в роли за патч (пустой patch = по всем): (games, wins).
    pub fn matchup_winrate(&self, patch: &str, role: &str, champion_id: i32, enemy_champion_id: i32) -> Result<(i64, i64), String> {
        let mut stmt = self
            .conn
            .prepare("SELECT COALESCE(SUM(games),0), COALESCE(SUM(wins),0) FROM matchup_agg WHERE (?1='' OR patch=?1) AND role=?2 AND champion_id=?3 AND enemy_champion_id=?4;")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![patch, role, champion_id, enemy_champion_id]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok((row.get(0).map_err(|e| e.to_string())?, row.get(1).map_err(|e| e.to_string())?))
        } else {
            Ok((0, 0))
        }
    }

    /// Кандидаты на роль (champion_id, games, wins) с минимумом игр за патч (пустой = все).
    pub fn role_candidates(&self, patch: &str, role: &str, min_games: i64) -> Result<Vec<(i32, i64, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT champion_id, SUM(games), SUM(wins) FROM champion_role_agg WHERE (?1='' OR patch=?1) AND role=?2 GROUP BY champion_id HAVING SUM(games) >= ?3;")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![patch, role, min_games], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Есть ли вообще данные матчапов (для решения, показывать ли рекомендации).
    // [NEEDS-USER] не подключён: решает пользователь — гейтить ли UI рекомендаций при пустой БД
    #[allow(dead_code)]
    pub fn has_matchup_data(&self) -> Result<bool, String> {
        Ok(self.count_where("SELECT COUNT(*) FROM champion_role_agg;")? > 0)
    }

    /// Винрейт синергии чемпиона в роли с союзником (суммарно по патчам): (games, wins).
    pub fn synergy_winrate(
        &self,
        patch: &str,
        role: &str,
        champion_id: i32,
        ally_role: &str,
        ally_champion_id: i32,
    ) -> Result<(i64, i64), String> {
        let mut stmt = self
            .conn
            .prepare("SELECT COALESCE(SUM(games),0), COALESCE(SUM(wins),0) FROM synergy_agg WHERE (?1='' OR patch=?1) AND role=?2 AND champion_id=?3 AND ally_role=?4 AND ally_champion_id=?5;")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt
            .query(params![patch, role, champion_id, ally_role, ally_champion_id])
            .map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            Ok((
                row.get(0).map_err(|e| e.to_string())?,
                row.get(1).map_err(|e| e.to_string())?,
            ))
        } else {
            Ok((0, 0))
        }
    }

    // ---------- Страница чемпиона ----------

    /// Роли чемпиона за патч (пустой = все): (role, games, wins), по убыванию игр.
    pub fn champion_roles(&self, patch: &str, champion_id: i32) -> Result<Vec<(String, i64, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT role, SUM(games), SUM(wins) FROM champion_role_agg WHERE (?1='' OR patch=?1) AND champion_id=?2 GROUP BY role ORDER BY SUM(games) DESC;")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![patch, champion_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Всего матчей за патч (пустой = все патчи) — знаменатель пик/бан-рейта.
    pub fn total_matches(&self, patch: &str) -> Result<i64, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT COALESCE(SUM(matches),0) FROM patch_totals WHERE (?1='' OR patch=?1);")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![patch]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(row.get(0).map_err(|e| e.to_string())?),
            None => Ok(0),
        }
    }

    /// Сколько раз чемпион сыгран за патч (пустой = все патчи, по всем ролям).
    pub fn champion_total_games(&self, patch: &str, champion_id: i32) -> Result<i64, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT COALESCE(SUM(games),0) FROM champion_role_agg WHERE (?1='' OR patch=?1) AND champion_id=?2;")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![patch, champion_id]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(row.get(0).map_err(|e| e.to_string())?),
            None => Ok(0),
        }
    }

    /// Сколько раз чемпион забанен за патч (пустой = все патчи).
    pub fn champion_ban_games(&self, patch: &str, champion_id: i32) -> Result<i64, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT COALESCE(SUM(games),0) FROM ban_agg WHERE (?1='' OR patch=?1) AND champion_id=?2;")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![patch, champion_id]).map_err(|e| e.to_string())?;
        match rows.next().map_err(|e| e.to_string())? {
            Some(row) => Ok(row.get(0).map_err(|e| e.to_string())?),
            None => Ok(0),
        }
    }

    /// Матчапы чемпиона в роли: (enemy_id, games, wins), по убыванию игр.
    pub fn champion_matchups(
        &self,
        patch: &str,
        role: &str,
        champion_id: i32,
        min_games: i64,
        limit: i64,
    ) -> Result<Vec<(i32, i64, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT enemy_champion_id, SUM(games), SUM(wins) FROM matchup_agg WHERE (?1='' OR patch=?1) AND role=?2 AND champion_id=?3 GROUP BY enemy_champion_id HAVING SUM(games) >= ?4 ORDER BY SUM(games) DESC LIMIT ?5;")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![patch, role, champion_id, min_games, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Синергии чемпиона в роли: (ally_role, ally_id, games, wins), по убыванию игр.
    pub fn champion_synergies(
        &self,
        patch: &str,
        role: &str,
        champion_id: i32,
        min_games: i64,
        limit: i64,
    ) -> Result<Vec<(String, i32, i64, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT ally_role, ally_champion_id, SUM(games), SUM(wins) FROM synergy_agg WHERE (?1='' OR patch=?1) AND role=?2 AND champion_id=?3 GROUP BY ally_role, ally_champion_id HAVING SUM(games) >= ?4 ORDER BY SUM(games) DESC LIMIT ?5;")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![patch, role, champion_id, min_games, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Предметы чемпиона в роли: (item_id, games, wins). is_first=1 — первый, 0 — финальный.
    pub fn champion_items(
        &self,
        patch: &str,
        role: &str,
        champion_id: i32,
        is_first: i32,
        min_games: i64,
        limit: i64,
    ) -> Result<Vec<(i32, i64, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT item_id, SUM(games), SUM(wins) FROM item_agg WHERE (?1='' OR patch=?1) AND role=?2 AND champion_id=?3 AND is_first=?4 GROUP BY item_id HAVING SUM(games) >= ?5 ORDER BY SUM(games) DESC LIMIT ?6;")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![patch, role, champion_id, is_first, min_games, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Предметы в конкретном слоте сборки (slot 0 = ботинки): (item_id, games, wins).
    pub fn champion_item_order(
        &self,
        patch: &str,
        role: &str,
        champion_id: i32,
        slot: i32,
        min_games: i64,
        limit: i64,
    ) -> Result<Vec<(i32, i64, i64)>, String> {
        let mut stmt = self
            .conn
            .prepare("SELECT item_id, SUM(games), SUM(wins) FROM item_order_agg WHERE (?1='' OR patch=?1) AND role=?2 AND champion_id=?3 AND slot=?4 GROUP BY item_id HAVING SUM(games) >= ?5 ORDER BY SUM(games) DESC LIMIT ?6;")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![patch, role, champion_id, slot, min_games, limit], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Топ рун чемпиона в роли за патч (пустой = все патчи): по убыванию игр в каждом виде.
    /// Возвращает все виды (keystone/primary/secondary) одним списком; фильтрация по виду — на стороне вызова.
    pub fn champion_runes(
        &self,
        patch: &str,
        role: &str,
        champion_id: i32,
        min_games: i64,
        limit_per_kind: i64,
    ) -> Result<Vec<RuneRow>, String> {
        // Для каждого вида берём топ-N по играм. Делаем отдельные запросы — limit на вид.
        let mut out: Vec<RuneRow> = Vec::new();
        for kind in ["keystone", "primary", "secondary"] {
            let mut stmt = self
                .conn
                .prepare("SELECT rune_id, SUM(games), SUM(wins) FROM rune_agg WHERE (?1='' OR patch=?1) AND role=?2 AND champion_id=?3 AND kind=?4 GROUP BY rune_id HAVING SUM(games) >= ?5 ORDER BY SUM(games) DESC LIMIT ?6;")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![patch, role, champion_id, kind, min_games, limit_per_kind], |r| {
                    Ok((r.get::<_, i32>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?))
                })
                .map_err(|e| e.to_string())?;
            for r in rows.filter_map(Result::ok) {
                out.push(RuneRow { kind: kind.to_string(), rune_id: r.0, games: r.1, wins: r.2 });
            }
        }
        Ok(out)
    }

    // ---------- Мета тир-лист (из champion_role_agg краулера) ----------

    /// Мета тир-лист из champion_role_agg. patch/role: пустая строка = без фильтра.
    pub fn meta_tier_list(&self, patch: &str, role: &str, min_games: i64, limit: i64) -> Result<Vec<TierRow>, String> {
        let mut stmt = self.conn.prepare(
            r#"
        SELECT champion_id, role, patch, SUM(games) AS g, SUM(wins) AS w
        FROM champion_role_agg
        WHERE (?1 = '' OR patch = ?1) AND (?2 = '' OR role = ?2)
        GROUP BY champion_id, role, patch
        HAVING SUM(games) >= ?3
        ORDER BY (CAST(SUM(wins) AS REAL) / SUM(games)) DESC, SUM(games) DESC
        LIMIT ?4
        "#,
        ).map_err(|e| e.to_string())?;
        let rows = stmt.query_map(params![patch, role, min_games, limit], |row| {
            Ok(TierRow {
                champion_id: row.get(0)?,
                role: row.get(1)?,
                patch: row.get(2)?,
                games: row.get(3)?,
                wins: row.get(4)?,
            })
        }).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn meta_tier_patches(&self) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT patch FROM champion_role_agg ORDER BY patch DESC LIMIT 30;").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get(0)).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn meta_tier_roles(&self) -> Result<Vec<String>, String> {
        let mut stmt = self.conn.prepare("SELECT DISTINCT role FROM champion_role_agg WHERE role <> '' ORDER BY role;").map_err(|e| e.to_string())?;
        let rows = stmt.query_map([], |row| row.get(0)).map_err(|e| e.to_string())?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    // ---------- кэш паттернов игрока ----------

    /// Возвращает закэшированный JSON-payload паттернов игрока, если он не старше
    /// `max_age_secs`. None — кэша нет или он протух (нужно пересчитать).
    pub fn get_player_pattern_cache(
        &self,
        puuid: &str,
        max_age_secs: i64,
    ) -> Result<Option<String>, String> {
        let now = now_unix();
        let mut stmt = self
            .conn
            .prepare("SELECT payload, computed_at FROM player_pattern_cache WHERE puuid = ?1;")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query(params![puuid]).map_err(|e| e.to_string())?;
        if let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let payload: String = row.get(0).map_err(|e| e.to_string())?;
            let computed_at: i64 = row.get(1).map_err(|e| e.to_string())?;
            if now - computed_at <= max_age_secs {
                return Ok(Some(payload));
            }
        }
        Ok(None)
    }

    /// Сохраняет (перезаписывает) кэш паттернов игрока.
    pub fn put_player_pattern_cache(
        &self,
        puuid: &str,
        games: i32,
        payload: &str,
    ) -> Result<(), String> {
        self.conn
            .execute(
                r#"
INSERT INTO player_pattern_cache (puuid, games, computed_at, payload)
VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(puuid) DO UPDATE SET
  games = excluded.games,
  computed_at = excluded.computed_at,
  payload = excluded.payload;
"#,
                params![puuid, games, now_unix(), payload],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    // [SAFE-TEST] parse_patch сравнивается ЧИСЛЕННО по кортежу (major, minor),
    // а не лексикографически по строке. Это инвариант, на котором держится
    // логика хранения/чистки патчей (какой патч новее).
    #[test]
    fn parse_patch_orders_numerically_not_lexically() {
        // Как строки "14.3" > "14.10", но патч 10 новее патча 3.
        assert!(parse_patch("14.3") < parse_patch("14.10"));
        // Переход через major: 15.24 старше 16.1.
        assert!(parse_patch("15.24") < parse_patch("16.1"));
        // Точные значения кортежа.
        assert_eq!(parse_patch("16.12"), (16, 12));
    }

    // [SAFE-TEST] Пробелы по краям компонент должны срезаться (trim), иначе
    // parse у "16 " упал бы и дал 0.
    #[test]
    fn parse_patch_trims_whitespace() {
        assert_eq!(parse_patch(" 16 . 12 "), (16, 12));
    }

    // [SAFE-TEST] Некорректный ввод деградирует предсказуемо в нули, а не паникует.
    #[test]
    fn parse_patch_malformed_degrades_to_zero() {
        assert_eq!(parse_patch(""), (0, 0));
        // Только major-компонента: minor отсутствует → 0.
        assert_eq!(parse_patch("16"), (16, 0));
        // Нечисловой ввод → (0, 0).
        assert_eq!(parse_patch("abc"), (0, 0));
        assert_eq!(parse_patch("16.x"), (16, 0));
    }
}


