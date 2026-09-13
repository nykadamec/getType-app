// gettype — historie přepisů (fáze 6).
//
// Uložení: `$HOME/Library/Application Support/gettype/history.json`
// (stejný vzor jako settings::config_path). Cap 50 záznamů, nejnovější
// první, staré se zahazují. Chybějící/corrupt soubor → prázdný seznam.
// Žádné paniky — chyby jen do stderr / Err(String) pro frontend.
//
// Stav drží `Mutex<History>` v managed state (stejný styl jako
// audio::Recorder). record() je synchronní a rychlé — volá se z stt.rs
// po úspěšné transkripci, před output::apply.
//
// Clipboard (copy_history_entry) běží VÝHRADNĚ na main threadu přes
// `AppHandle::run_on_main_thread` — NSPasteboard mimo main thread
// segfaultuje celý proces (viz output.rs).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

/// Maximum uložených záznamů (nejnovější první).
const MAX_ENTRIES: usize = 50;

/// Monotonní counter pro unikátnost id v rámci jedné milisekundy
/// (unix_ms + counter). Proces-škálový, po restartu se resetuje —
/// kolize řeší kontrola proti existujícím id v record().
static ID_COUNTER: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub text: String,
    pub created_at: i64,
    pub chars: usize,
    pub model: String,
    #[serde(default)]
    pub ai_action: Option<String>,
    #[serde(default)]
    pub raw_text: Option<String>,
}

/// Managed state: `app.manage(Mutex::new(History::default()))`.
#[derive(Debug, Default)]
pub struct History {
    entries: Vec<Entry>,
}

impl History {
    /// Default načte persistovaná data z disku (chybějící/corrupt → prázdné).
    pub fn load() -> Self {
        Self {
            entries: load_from_disk(),
        }
    }
}

/// `$HOME/Library/Application Support/gettype/history.json`
fn history_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("gettype")
            .join("history.json"),
    )
}

/// Chybějící/corrupt soubor → prázdný seznam. Nikdy nepanikuje.
fn load_from_disk() -> Vec<Entry> {
    let Some(path) = history_path() else {
        return Vec::new();
    };
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };
    let mut entries: Vec<Entry> = serde_json::from_str(&content).unwrap_or_default();
    entries.truncate(MAX_ENTRIES);
    entries
}

fn save_to_disk(entries: &[Entry]) -> Result<(), String> {
    let path = history_path().ok_or_else(|| "$HOME is not set".to_string())?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("create history dir failed: {e}"))?;
    }
    let json = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("write history failed: {e}"))
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Zaznamená úspěšnou transkripci. Voláno synchronně z stt.rs před
/// output::apply. Chyby jen do stderr — nikdy nepanikuje, výsledek
/// transkripce tím není ohrožen.
///
/// `text` je finální text (po případném AI post-processu).
/// `raw_text` je Some(původní STT přepis) jen když proběhlo AI
/// a výsledek se liší od raw; jinak None (žádný badge, žádný duplikát).
/// `ai_action` je Some("cleanup"|"summarize"|"translate_en") jen při
/// úspěšném AI — jinak None (fallback/passthrough → bez badge).
pub fn record(
    app: &AppHandle,
    text: &str,
    raw_text: Option<&str>,
    model: &str,
    ai_action: Option<&str>,
) {
    let ms = now_ms();
    let count = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut id = format!("{ms}-{count}");
    let now = now_secs();

    let Some(state) = app.try_state::<std::sync::Mutex<History>>() else {
        crate::log::warn("history", "record skipped reason=\"state not managed\"");
        return;
    };
    // Poisoned mutex → stále použijeme vnitřek, historii nezahodíme.
    let mut history = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };

    // Kolize id (stejná ms po restartu procesu) → bumpuj counter do unikátnosti.
    while history.entries.iter().any(|e| e.id == id) {
        let extra = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
        id = format!("{ms}-{extra}");
    }

    history.entries.insert(
        0,
        Entry {
            id,
            text: text.to_string(),
            created_at: now,
            chars: text.chars().count(),
            model: model.to_string(),
            ai_action: ai_action.map(|s| s.to_string()),
            raw_text: raw_text.map(|s| s.to_string()),
        },
    );
    history.entries.truncate(MAX_ENTRIES);
    if let Err(e) = save_to_disk(&history.entries) {
        crate::log::error("history", format!("save failed err=\"{e}\""));
    }
}

/// Nejnovější první.
#[tauri::command]
pub fn list_history(app: AppHandle) -> Result<Vec<Entry>, String> {
    let state = app
        .try_state::<std::sync::Mutex<History>>()
        .ok_or_else(|| "history state not initialized".to_string())?;
    let history = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    Ok(history.entries.clone())
}

/// Najde text podle id a nastaví clipboard — POVINNĚ na main threadu
/// (NSPasteboard mimo main thread = segfault, viz output.rs).
#[tauri::command]
pub fn copy_history_entry(app: AppHandle, id: String) -> Result<(), String> {
    let text = {
        let state = app
            .try_state::<std::sync::Mutex<History>>()
            .ok_or_else(|| "history state not initialized".to_string())?;
        let history = match state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        history
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.text.clone())
            .ok_or_else(|| "entry not found".to_string())?
    };
    app.run_on_main_thread(move || {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => {
                if let Err(e) = clipboard.set_text(&text) {
                    crate::log::error("history", format!("copy failed err=\"{e}\""));
                } else {
                    crate::log::info("history", "copy ok=true");
                }
            }
            Err(e) => crate::log::error("history", format!("clipboard open failed err=\"{e}\"")),
        }
    })
    .map_err(|e| e.to_string())
}

/// Smaže záznam, vrátí nový seznam (nejnovější první).
#[tauri::command]
pub fn delete_history_entry(app: AppHandle, id: String) -> Result<Vec<Entry>, String> {
    let state = app
        .try_state::<std::sync::Mutex<History>>()
        .ok_or_else(|| "history state not initialized".to_string())?;
    let mut history = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    let before = history.entries.len();
    history.entries.retain(|e| e.id != id);
    if history.entries.len() == before {
        return Err("entry not found".to_string());
    }
    save_to_disk(&history.entries)?;
    Ok(history.entries.clone())
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<std::sync::Mutex<History>>()
        .ok_or_else(|| "history state not initialized".to_string())?;
    let mut history = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    history.entries.clear();
    save_to_disk(&history.entries)
}

#[cfg(test)]
mod tests {
    use super::Entry;

    /// Staré history.json (v0.1.1, bez ai_action/raw_text) se musí
    /// načíst bez chyby, chybějící pole → None.
    #[test]
    fn old_entries_default_to_none() {
        let json = r#"[{"id":"1","text":"ahoj","created_at":123,"chars":4,"model":"whisper-large-v3-turbo"}]"#;
        let entries: Vec<Entry> = serde_json::from_str(json).expect("old json parses");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].ai_action.is_none());
        assert!(entries[0].raw_text.is_none());
    }

    /// Nové záznamy s AI badgem projdou roundtripem.
    #[test]
    fn new_entries_roundtrip() {
        let json = r#"[{"id":"2","text":"Cleaned up.","created_at":456,"chars":11,"model":"whisper-large-v3-turbo","ai_action":"cleanup","raw_text":"ehm cleaned up"}]"#;
        let entries: Vec<Entry> = serde_json::from_str(json).expect("new json parses");
        assert_eq!(entries[0].ai_action.as_deref(), Some("cleanup"));
        assert_eq!(entries[0].raw_text.as_deref(), Some("ehm cleaned up"));
        let back = serde_json::to_string(&entries).expect("serialize");
        let again: Vec<Entry> = serde_json::from_str(&back).expect("re-parse");
        assert_eq!(again[0].ai_action.as_deref(), Some("cleanup"));
    }
}
