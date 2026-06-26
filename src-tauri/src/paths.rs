use std::path::PathBuf;
use std::sync::OnceLock;

/// Папка данных приложения (app_data_dir). Инициализируется один раз в setup().
/// До инициализации (ранний старт, паника до setup) используется current_dir,
/// чтобы не терять логи.
static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    let _ = DATA_DIR.set(dir);
}

pub fn data_dir() -> PathBuf {
    DATA_DIR
        .get()
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn db_path() -> PathBuf {
    data_dir().join("lol_stats.db")
}

pub fn log_path() -> PathBuf {
    data_dir().join("lol_draft_assistant_log.txt")
}

pub fn league_path_file() -> PathBuf {
    data_dir().join("lol_draft_assistant_league_path.txt")
}

pub fn ddragon_cache_dir() -> PathBuf {
    let p = data_dir().join("ddragon");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn lcu_debug_file() -> PathBuf {
    data_dir().join("lcu_session_debug.json")
}

/// Переносит файлы из старых расположений (current_dir и папка с exe) в app_data_dir.
/// Копирование, не перемещение — безопаснее. Вызывается один раз после init().
pub fn migrate_legacy_files() {
    let legacy_dirs: Vec<PathBuf> = [
        std::env::current_dir().ok(),
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|p| p.to_path_buf())),
    ]
    .into_iter()
    .flatten()
    .collect();

    let targets = [
        ("lol_stats.db", db_path()),
        ("lol_draft_assistant_league_path.txt", league_path_file()),
    ];

    for (name, dst) in &targets {
        if dst.exists() {
            continue;
        }
        for dir in &legacy_dirs {
            let src = dir.join(name);
            if src.exists() && src != *dst
                && std::fs::copy(&src, dst).is_ok() {
                    break;
                }
        }
    }
}
