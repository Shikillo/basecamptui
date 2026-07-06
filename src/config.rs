use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("settlement");
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("settlement");
        }
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local")
        .join("share")
        .join("settlement")
}

pub fn staged_path() -> PathBuf {
    data_dir().join("staged.json")
}

pub fn recording_cache_path() -> PathBuf {
    data_dir().join("recording_cache.json")
}

pub fn favorites_path() -> PathBuf {
    data_dir().join("favorites.json")
}

pub fn today_iso() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
