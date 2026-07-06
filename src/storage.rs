use std::collections::{HashMap, HashSet};
use crate::config::{favorites_path, recording_cache_path, staged_path};
use crate::models::StagedEntry;

pub fn load_staged() -> Vec<StagedEntry> {
    let path = staged_path();
    let Ok(text) = std::fs::read_to_string(&path) else { return vec![] };
    let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    val["entries"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn save_staged(entries: &[StagedEntry]) {
    let path = staged_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let payload = serde_json::json!({ "entries": entries });
    let _ = std::fs::write(&path, serde_json::to_string_pretty(&payload).unwrap_or_default());
}

pub fn add_staged(entry: StagedEntry) -> Vec<StagedEntry> {
    let mut entries = load_staged();
    entries.push(entry);
    save_staged(&entries);
    entries
}

pub fn remove_staged(entry_id: &str) -> Vec<StagedEntry> {
    let entries: Vec<_> = load_staged().into_iter().filter(|e| e.id != entry_id).collect();
    save_staged(&entries);
    entries
}

pub fn load_favorites() -> HashSet<i64> {
    let path = favorites_path();
    let Ok(text) = std::fs::read_to_string(&path) else { return HashSet::new() };
    serde_json::from_str::<Vec<i64>>(&text)
        .unwrap_or_default()
        .into_iter()
        .collect()
}

pub fn save_favorites(favorites: &HashSet<i64>) {
    let path = favorites_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut sorted: Vec<i64> = favorites.iter().copied().collect();
    sorted.sort();
    let _ = std::fs::write(&path, serde_json::to_string(&sorted).unwrap_or_default());
}

pub fn toggle_favorite(project_id: i64) -> HashSet<i64> {
    let mut favs = load_favorites();
    if favs.contains(&project_id) {
        favs.remove(&project_id);
    } else {
        favs.insert(project_id);
    }
    save_favorites(&favs);
    favs
}

pub fn load_recording_cache() -> HashMap<String, i64> {
    let path = recording_cache_path();
    let Ok(text) = std::fs::read_to_string(&path) else { return HashMap::new() };
    serde_json::from_str(&text).unwrap_or_default()
}

pub fn save_recording_cache(cache: &HashMap<String, i64>) {
    let path = recording_cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&path, serde_json::to_string(cache).unwrap_or_default());
}
