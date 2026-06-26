use reqwest::blocking::Client;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

/// Метаданные чемпиона из Data Dragon.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChampionMeta {
    pub id: u32,
    /// Имя файла иконки (например "MonkeyKing" для Вуконга) — не зависит от локали.
    pub icon_name: String,
    pub name: String,
    pub tags: Vec<String>,
    pub attack: u8,
    pub magic: u8,
    pub defense: u8,
}

pub struct ChampionCatalog {
    pub version: String,
    pub by_id: HashMap<u32, ChampionMeta>,
}

pub type SharedCatalog = Arc<RwLock<ChampionCatalog>>;

/// Глобальная ссылка на каталог — для свободных функций (lcu.rs), которым
/// неудобно протаскивать State.
static GLOBAL: OnceLock<SharedCatalog> = OnceLock::new();

pub fn set_global(catalog: SharedCatalog) {
    let _ = GLOBAL.set(catalog);
}

/// Текущая версия каталога (= патч), если каталог инициализирован.
pub fn current_version() -> Option<String> {
    GLOBAL.get().and_then(|s| s.read().ok()).map(|c| c.version.clone())
}

/// Полные метаданные чемпиона по ID (None — каталог не готов или ID неизвестен).
pub fn champion_meta(id: u32) -> Option<ChampionMeta> {
    let shared = GLOBAL.get()?;
    let cat = shared.read().ok()?;
    cat.by_id.get(&id).cloned()
}

/// Имя чемпиона по ID. До инициализации каталога или для неизвестных ID — "ID {id}".
pub fn display_name(id: u32) -> String {
    if let Some(shared) = GLOBAL.get() {
        if let Ok(cat) = shared.read() {
            if let Some(meta) = cat.by_id.get(&id) {
                return meta.name.clone();
            }
        }
    }
    format!("ID {}", id)
}

// ---------- Парсинг champion.json ----------

#[derive(serde::Deserialize)]
struct ChampionJson {
    version: String,
    data: HashMap<String, ChampionEntry>,
}

#[derive(serde::Deserialize)]
struct ChampionEntry {
    /// Числовой ID в виде строки ("266").
    key: String,
    /// Имя файла иконки ("Aatrox", "MonkeyKing").
    id: String,
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    info: ChampionInfo,
}

#[derive(serde::Deserialize, Default)]
struct ChampionInfo {
    #[serde(default)]
    attack: u8,
    #[serde(default)]
    magic: u8,
    #[serde(default)]
    defense: u8,
}

pub fn parse_champion_json(body: &str) -> Result<ChampionCatalog, String> {
    let parsed: ChampionJson = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let mut by_id = HashMap::new();
    for (_, entry) in parsed.data {
        let Ok(id) = entry.key.parse::<u32>() else { continue };
        by_id.insert(
            id,
            ChampionMeta {
                id,
                icon_name: entry.id,
                name: entry.name,
                tags: entry.tags,
                attack: entry.info.attack,
                magic: entry.info.magic,
                defense: entry.info.defense,
            },
        );
    }
    if by_id.is_empty() {
        return Err("champion.json: пустой список чемпионов".into());
    }
    Ok(ChampionCatalog {
        version: parsed.version,
        by_id,
    })
}

// ---------- Загрузка: бандл → кэш → сеть ----------

/// Снапшот, вшитый в бинарник (обновляется вручную при сборке).
pub fn bundled() -> ChampionCatalog {
    let body = include_str!("../resources/champion_snapshot.json");
    parse_champion_json(body).expect("bundled champion_snapshot.json is invalid")
}

fn cache_file(cache_dir: &Path) -> std::path::PathBuf {
    cache_dir.join("champion.json")
}

pub fn load_from_cache(cache_dir: &Path) -> Option<ChampionCatalog> {
    let body = std::fs::read_to_string(cache_file(cache_dir)).ok()?;
    parse_champion_json(&body).ok()
}

/// Множество ID «завершённых» предметов (для определения первого собранного):
/// финальные предметы (не компоненты) дороже 1100 золота.
pub fn fetch_completed_items(client: &Client, version: &str) -> std::collections::HashSet<i32> {
    #[derive(serde::Deserialize)]
    struct ItemGold { #[serde(default)] total: i32, #[serde(default)] purchasable: bool }
    #[derive(serde::Deserialize)]
    struct ItemEntry {
        #[serde(default)] gold: Option<ItemGold>,
        #[serde(default)] into: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct ItemJson { data: std::collections::HashMap<String, ItemEntry> }

    let url = format!(
        "https://ddragon.leagueoflegends.com/cdn/{}/data/ru_RU/item.json",
        version
    );
    let mut set = std::collections::HashSet::new();
    let body = match client.get(&url).send().and_then(|r| r.text()) {
        Ok(b) => b,
        Err(_) => return set,
    };
    let parsed: ItemJson = match serde_json::from_str(&body) {
        Ok(p) => p,
        Err(_) => return set,
    };
    for (id, entry) in parsed.data {
        let Ok(id) = id.parse::<i32>() else { continue };
        let gold = entry.gold.unwrap_or(ItemGold { total: 0, purchasable: false });
        // Финальный предмет: ни во что не апгрейдится, дорогой, покупаемый.
        if entry.into.is_empty() && gold.purchasable && gold.total >= 1100 {
            set.insert(id);
        }
    }
    set
}

/// Иконка + имя + описание руны/древа.
#[derive(Clone, Default)]
pub struct RuneInfo {
    pub icon: String,
    pub name: String,
    pub desc: String,
}

/// Кэш рун: id руны/древа → инфо. Грузится лениво при первом разборе матча;
/// иконки рун в Data Dragon версионно-независимы.
static RUNES: OnceLock<RwLock<HashMap<i32, RuneInfo>>> = OnceLock::new();

pub fn runes_cache() -> &'static RwLock<HashMap<i32, RuneInfo>> {
    RUNES.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Удаляет HTML-теги из описания руны (shortDesc приходит с <br>, <b> и т.п.).
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Карта perkId/styleId → инфо из runesReforged.json (древа и сами руны) + осколки.
pub fn fetch_rune_catalog(client: &Client, version: &str) -> HashMap<i32, RuneInfo> {
    #[derive(serde::Deserialize)]
    struct RuneDto {
        id: i32,
        #[serde(default)]
        icon: String,
        #[serde(default)]
        name: String,
        #[serde(rename = "shortDesc", default)]
        short_desc: String,
    }
    #[derive(serde::Deserialize)]
    struct SlotDto {
        #[serde(default)]
        runes: Vec<RuneDto>,
    }
    #[derive(serde::Deserialize)]
    struct StyleDto {
        id: i32,
        #[serde(default)]
        icon: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        slots: Vec<SlotDto>,
    }

    let url = format!(
        "https://ddragon.leagueoflegends.com/cdn/{}/data/ru_RU/runesReforged.json",
        version
    );
    let mut map: HashMap<i32, RuneInfo> = HashMap::new();
    let body = match client.get(&url).send().and_then(|r| r.text()) {
        Ok(b) => b,
        Err(_) => return map,
    };
    let styles: Vec<StyleDto> = match serde_json::from_str(&body) {
        Ok(s) => s,
        Err(_) => return map,
    };
    for s in styles {
        map.insert(
            s.id,
            RuneInfo {
                icon: s.icon,
                name: s.name,
                desc: String::new(),
            },
        );
        for slot in s.slots {
            for r in slot.runes {
                map.insert(
                    r.id,
                    RuneInfo {
                        icon: r.icon,
                        name: r.name,
                        desc: strip_html(&r.short_desc),
                    },
                );
            }
        }
    }

    // Осколки статов (statPerks) в runesReforged.json отсутствуют — иконки лежат по
    // фиксированным путям perk-images/StatMods; имена задаём вручную. Если иконка
    // 404 — фронт просто не покажет картинку, без ошибки.
    let stat_mods: [(i32, &str, &str); 9] = [
        (5001, "StatModsHealthScalingIcon.png", "Здоровье (рост)"),
        (5002, "StatModsArmorIcon.png", "Броня"),
        (5003, "StatModsMagicResIcon.png", "Сопротивление магии"),
        (5005, "StatModsAttackSpeedIcon.png", "Скорость атаки"),
        (5007, "StatModsCDRScalingIcon.png", "Ускорение умений"),
        (5008, "StatModsAdaptiveForceIcon.png", "Адаптивная сила"),
        (5010, "StatModsMovementSpeedIcon.png", "Скорость передвижения"),
        (5011, "StatModsHealthPlusIcon.png", "Здоровье"),
        (5013, "StatModsTenacityIcon.png", "Стойкость и замедления"),
    ];
    for (id, file, name) in stat_mods {
        map.entry(id).or_insert_with(|| RuneInfo {
            icon: format!("perk-images/StatMods/{}", file),
            name: name.to_string(),
            desc: String::new(),
        });
    }
    map
}

/// Множество ID ботинок (предметы с тегом "Boots", завершённые: ни во что не апгрейдятся).
pub fn fetch_boots_items(client: &Client, version: &str) -> std::collections::HashSet<i32> {
    #[derive(serde::Deserialize)]
    struct ItemEntry {
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        into: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct ItemJson {
        data: std::collections::HashMap<String, ItemEntry>,
    }
    let url = format!(
        "https://ddragon.leagueoflegends.com/cdn/{}/data/ru_RU/item.json",
        version
    );
    let mut set = std::collections::HashSet::new();
    let body = match client.get(&url).send().and_then(|r| r.text()) {
        Ok(b) => b,
        Err(_) => return set,
    };
    let parsed: ItemJson = match serde_json::from_str(&body) {
        Ok(p) => p,
        Err(_) => return set,
    };
    for (id, entry) in parsed.data {
        let Ok(id) = id.parse::<i32>() else { continue };
        // Завершённые ботинки: тег Boots и не апгрейдятся дальше (исключаем base-ботинки 1001).
        if entry.tags.iter().any(|t| t == "Boots") && entry.into.is_empty() {
            set.insert(id);
        }
    }
    set
}

/// Обновление из сети: versions.json → champion.json (ru_RU). Пишет в кэш.
pub fn refresh_from_network(client: &Client, cache_dir: &Path) -> Result<ChampionCatalog, String> {
    let versions: Vec<String> = client
        .get("https://ddragon.leagueoflegends.com/api/versions.json")
        .send()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let latest = versions.first().ok_or("versions.json пуст")?;
    let url = format!(
        "https://ddragon.leagueoflegends.com/cdn/{}/data/ru_RU/champion.json",
        latest
    );
    let body = client
        .get(&url)
        .send()
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())?;
    let catalog = parse_champion_json(&body)?;
    let _ = std::fs::write(cache_file(cache_dir), &body);
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshot_parses() {
        let cat = bundled();
        assert!(cat.by_id.len() > 150, "ожидалось >150 чемпионов, есть {}", cat.by_id.len());
        // Малфит (id 54) должен быть в каталоге, иконка locale-независимая
        let malphite = cat.by_id.get(&54).expect("Malphite (54) отсутствует");
        assert_eq!(malphite.icon_name, "Malphite");
        assert!(!malphite.name.is_empty());
        // Вуконг: id поля иконки отличается от имени
        let wukong = cat.by_id.get(&62).expect("Wukong (62) отсутствует");
        assert_eq!(wukong.icon_name, "MonkeyKing");
    }

    #[test]
    fn parse_minimal() {
        let json = r#"{"version":"16.12.1","data":{"Ahri":{"key":"103","id":"Ahri","name":"Ари","tags":["Mage","Assassin"],"info":{"attack":3,"defense":4,"magic":8,"difficulty":5}}}}"#;
        let cat = parse_champion_json(json).unwrap();
        assert_eq!(cat.version, "16.12.1");
        let ahri = cat.by_id.get(&103).unwrap();
        assert_eq!(ahri.name, "Ари");
        assert_eq!(ahri.magic, 8);
        assert_eq!(ahri.tags, vec!["Mage", "Assassin"]);
    }

    // [SAFE-TEST] strip_html: символы между '<' и '>' выбрасываются, затем
    // последовательности пробелов схлопываются в один (split_whitespace+join).
    // ВАЖНО: теги НЕ заменяются пробелом — "</b><br>" просто исчезают, поэтому
    // соседний текст склеивается ("Deal" + "10" → "Deal10").
    #[test]
    fn strip_html_removes_tags_and_collapses_whitespace() {
        assert_eq!(strip_html("<b>Deal</b><br>10 dmg"), "Deal10 dmg");
        // Несколько пробелов/перевод строки между словами → один пробел.
        assert_eq!(strip_html("a   <i>b</i>\n  c"), "a b c");
        // Ведущие/хвостовые пробелы срезаются.
        assert_eq!(strip_html("  <b>hi</b>  "), "hi");
    }

    #[test]
    fn strip_html_handles_nested_and_unclosed_tags() {
        // Вложенные теги: содержимое сохраняется, разметка удаляется.
        assert_eq!(strip_html("<a><b>X</b> Y</a>"), "X Y");
        // Незакрытый тег: всё после '<' и до конца строки трактуется как тег.
        assert_eq!(strip_html("text <unclosed"), "text");
        // Пустой ввод.
        assert_eq!(strip_html(""), "");
    }

    // [SAFE-TEST] Ключ-нечисло (entry.key не парсится в u32) пропускается, а не
    // роняет весь каталог.
    #[test]
    fn parse_champion_json_skips_non_numeric_key() {
        let json = r#"{"version":"16.12.1","data":{
            "Ahri":{"key":"103","id":"Ahri","name":"Ари","tags":[],"info":{}},
            "Broken":{"key":"none","id":"Broken","name":"Сломан","tags":[],"info":{}}
        }}"#;
        let cat = parse_champion_json(json).unwrap();
        assert!(cat.by_id.contains_key(&103));
        // Запись с нечисловым key отброшена → только один чемпион.
        assert_eq!(cat.by_id.len(), 1);
    }

    // [SAFE-TEST] Пустой data → Err (нет смысла в каталоге без чемпионов).
    #[test]
    fn parse_champion_json_empty_data_is_err() {
        let json = r#"{"version":"16.12.1","data":{}}"#;
        assert!(parse_champion_json(json).is_err());
    }
}
