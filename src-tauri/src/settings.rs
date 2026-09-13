// gettype — persisted settings (config.json) + Groq API key (macOS Keychain)

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

pub const KEYCHAIN_SERVICE: &str = "com.gettype.app";
pub const KEYCHAIN_ACCOUNT: &str = "groq_api_key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub hotkey: String,
    pub mode: String,
    pub auto_paste: bool,
    pub copy_clipboard: bool,
    pub model: String,
    pub language: String,
    pub theme: String,
    pub ai_enabled: bool,
    pub ai_default_action: String,
    pub ai_model: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: "Option+Space".into(),
            mode: "push_to_talk".into(),
            auto_paste: true,
            copy_clipboard: true,
            model: "whisper-large-v3-turbo".into(),
            language: "cs".into(),
            theme: "system".into(),
            ai_enabled: false,
            ai_default_action: "cleanup".into(),
            ai_model: "llama-3.1-8b-instant".into(),
        }
    }
}

/// Payload returned to the settings window on load.
#[derive(Serialize)]
pub struct LoadedSettings {
    pub config: Config,
    pub has_api_key: bool,
}

/// `$HOME/Library/Application Support/gettype/config.json`
pub fn config_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("gettype")
            .join("config.json"),
    )
}

/// Missing or corrupt file → defaults. Partial file → missing fields default in.
///
/// Čtení jde přes in-memory cache (Fáze B5): `load()` se volá při každé
/// dikci (stt + output), čtení z disku pokaždé je zbytečné. Invalidace v `save()`.
fn config_cache() -> &'static RwLock<Config> {
    static CACHE: OnceLock<RwLock<Config>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(load_from_disk()))
}

pub fn load() -> Config {
    config_cache()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
}

fn load_from_disk() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

pub fn save(config: &Config) -> Result<(), String> {
    let path = config_path().ok_or_else(|| "$HOME is not set".to_string())?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("create config dir failed: {e}"))?;
    }
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("write config failed: {e}"))?;
    // Invalidace cache — odteď čte `load()` novou hodnotu.
    *config_cache()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = config.clone();
    Ok(())
}

fn keychain_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(|e| e.to_string())
}

/// Ok(None) when no entry exists; Err on real keychain failure.
///
/// Úspěšné čtení se cachuje v paměti (Fáze B5) — Keychain IPC při každé
/// dikci je zbytečné. Chyby se necacheují (můžou být přechodné).
/// Invalidace v set/delete. Cache nikdy neopouští proces, nikdy se neloguje.
fn api_key_cache() -> &'static RwLock<Option<Option<String>>> {
    static CACHE: OnceLock<RwLock<Option<Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(None))
}

pub fn get_api_key() -> Result<Option<String>, String> {
    if let Some(cached) = api_key_cache()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or(None)
    {
        return Ok(cached);
    }
    let result = read_api_key()?;
    *api_key_cache()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(result.clone());
    Ok(result)
}

fn read_api_key() -> Result<Option<String>, String> {
    let entry = keychain_entry()?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// SECURITY: vrací plaintext klíč z Keychainu pouze do lokálního Settings
/// okna na explicitní vyžádání uživatelem (oko) — momentálně používaný klíč.
/// Nikdy nelogovat, necachovat na frontend mimo input, neposílat po síti.
#[tauri::command]
pub fn reveal_api_key() -> Result<Option<String>, String> {
    get_api_key()
}

/// Maskovaný tvar skutečného klíče z Keychainu pro status/placeholder.
/// `None` když klíč není. Jinak `"gsk_••••" + poslední 4 znaky`;
/// pro klíč kratší než 8 znaků jen `"••••" + poslední znaky`
/// (u klíče ≤ 4 znaky jen `"••••"` — nikdy nevrací celý klíč).
#[tauri::command]
pub fn masked_api_key() -> Result<Option<String>, String> {
    match get_api_key()? {
        None => Ok(None),
        Some(key) => {
            if key.len() < 8 {
                if key.len() <= 4 {
                    Ok(Some("••••".to_string()))
                } else {
                    let tail: String = key.chars().rev().take(4).collect::<String>().chars().rev().collect();
                    Ok(Some(format!("••••{tail}")))
                }
            } else {
                let tail: String = key.chars().rev().take(4).collect::<String>().chars().rev().collect();
                Ok(Some(format!("gsk_••••{tail}")))
            }
        }
    }
}

pub fn set_api_key(key: &str) -> Result<(), String> {
    keychain_entry()?
        .set_password(key)
        .map_err(|e| e.to_string())?;
    *api_key_cache()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Some(key.to_string()));
    Ok(())
}

/// Deleting a non-existent entry is treated as success.
pub fn delete_api_key() -> Result<(), String> {
    let entry = keychain_entry()?;
    match entry.delete_credential() {
        Ok(()) => {}
        Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(e.to_string()),
    }
    *api_key_cache()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(None);
    Ok(())
}
