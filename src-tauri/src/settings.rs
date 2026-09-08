// gettype — persisted settings (config.json) + Groq API key (macOS Keychain)

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
pub fn load() -> Config {
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
    fs::write(&path, json).map_err(|e| format!("write config failed: {e}"))
}

fn keychain_entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT).map_err(|e| e.to_string())
}

/// Ok(None) when no entry exists; Err on real keychain failure.
pub fn get_api_key() -> Result<Option<String>, String> {
    let entry = keychain_entry()?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub fn set_api_key(key: &str) -> Result<(), String> {
    keychain_entry()?
        .set_password(key)
        .map_err(|e| e.to_string())
}

/// Deleting a non-existent entry is treated as success.
pub fn delete_api_key() -> Result<(), String> {
    let entry = keychain_entry()?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
