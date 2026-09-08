// gettype — výstup transkripce: clipboard + auto-paste (⌘V) (fáze 5).
//
// Paste jde přes clipboard (nastavíme text, simulujeme ⌘V). Když uživatel
// Copy nechce, po pastu vrátíme původní obsah schránky. Chyby jen zalogujeme
// do stderr — žádné paniky.

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;
use tauri::AppHandle;

use crate::settings;

/// Prodleva mezi nastavením clipboardu a simulací ⌘V (ať si appky stihnou
/// clipboard vyzvednout).
const PASTE_DELAY: Duration = Duration::from_millis(80);
/// Prodleva po pastu, než obnovíme původní clipboard.
const RESTORE_DELAY: Duration = Duration::from_millis(250);

/// Aplikuje výsledek dle configu (auto_paste, copy_clipboard).
///
/// Skrytí pilulky je vždy první krok — paste nesmí proběhnout, dokud je
/// okno viditelné. Fade-out je v pořádku: okno nikdy nemá focus, ⌘V jde do
/// cílové appky (CSS ztrácí opacity, paste delay 80 ms zůstává); reálné
/// window.hide() přichází až po fade animaci z pill::fade_out.
pub fn apply(app: &AppHandle, text: String) {
    crate::pill::fade_out(app);

    let config = settings::load();

    // Auto-paste vypnuté: buď jen clipboard (Copy zapnuto), nebo nic.
    if !config.auto_paste {
        if config.copy_clipboard {
            if let Err(e) = set_clipboard(&text) {
                eprintln!("gettype: clipboard write failed: {e}");
            }
        }
        return;
    }

    // Auto-paste zapnuté: 1) schovej starý obsah schránky, 2) nastav nový,
    // 3) ⌘V s malým odstupem, 4) podle Copy obnov původní obsah.
    let previous = clipboard_text();

    if let Err(e) = set_clipboard(&text) {
        eprintln!("gettype: clipboard write failed: {e}");
        return; // paste by vložil starý obsah — radši se kývl k nicnedělání
    }

    let copy_clipboard = config.copy_clipboard;
    tauri::async_runtime::spawn(async move {
        // Async sleep — čekání blokuje jen tento task, ne executor.
        tokio::time::sleep(PASTE_DELAY).await;
        if let Err(e) = paste_text() {
            eprintln!("gettype: paste failed: {e}");
        }
        tokio::time::sleep(RESTORE_DELAY).await;
        if !copy_clipboard {
            if let Some(old) = previous {
                if let Err(e) = set_clipboard(&old) {
                    eprintln!("gettype: clipboard restore failed: {e}");
                }
            }
        }
    });
}

/// Simulace ⌘V: ⌘ stisknout → V kliknout → ⌘ uvolnit (API enigo 0.6).
fn paste_text() -> Result<(), String> {
    // open_prompt_to_get_permissions: false — chybějící Accessibility
    // permission nesmí vyskakovat dialogem při každé dikci; jen stderr.
    let settings = Settings {
        open_prompt_to_get_permissions: false,
        ..Settings::default()
    };
    let mut enigo = Enigo::new(&settings).map_err(|e| {
        format!(
            "input simulation unavailable (Accessibility permission?): {e}"
        )
    })?;
    enigo.key(Key::Meta, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| e.to_string())?;
    enigo.key(Key::Meta, Direction::Release)
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn clipboard_text() -> Option<String> {
    Clipboard::new().ok()?.get_text().ok()
}

fn set_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}
