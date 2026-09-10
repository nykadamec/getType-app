// gettype — výstup transkripce: clipboard + auto-paste (⌘V) (fáze 5).
//
// Paste jde přes clipboard (nastavíme text, simulujeme ⌘V). Když uživatel
// Copy nechce, po pastu vrátíme původní obsah schránky. Chyby jen zalogujeme
// do stderr — žádné paniky.
//
// Vše, co sahá na NSPasteboard / CGEvent (clipboard + enigo), běží VÝHRADNĚ
// na main threadu přes `AppHandle::run_on_main_thread` — volání z async
// tasku mimo main thread segfaultuje celý proces (tichý pád bez paniky).
// Jeden `Clipboard` handle na celou apply (read previous + set + restore).

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;
use tauri::AppHandle;

use crate::settings;
use crate::log;

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
/// Jediný owner skrývání pilulky — stt.rs po apply žádný další fade nevolá.
///
/// Po cancelu (`generation` je stale) nic nezapisuje: ani clipboard, ani ⌘V
/// (enigo). Kontroluje se na vstupu i těsně před pastem na main threadu
/// (cancel mohl přijít během PASTE_DELAY).
pub fn apply(app: &AppHandle, text: String, generation: u64) {
    if crate::audio::is_stale(generation) {
        log::info("output", format!("apply skipped stale=true generation={generation}"));
        return;
    }
    log::info("output", format!("apply start chars={} generation={generation}", text.chars().count()));
    crate::pill::fade_out(app);

    let config = settings::load();

    // Auto-paste vypnuté: buď jen clipboard (Copy zapnuto), nebo nic.
    // Clipboard i tady patří na main thread (NSPasteboard).
    if !config.auto_paste {
        if config.copy_clipboard {
            let handle = app.clone();
            if let Err(e) = handle.run_on_main_thread(move || {
                if crate::audio::is_stale(generation) {
                    log::info("output", format!("clipboard set skipped stale=true generation={generation}"));
                    return;
                }
                match Clipboard::new() {
                    Ok(mut clipboard) => match clipboard.set_text(&text) {
                        Ok(()) => log::info("output", "clipboard set ok=true copy-only=true"),
                        Err(e) => log::error("output", format!("clipboard write failed copy-only=true err=\"{e}\"")),
                    },
                    Err(e) => log::error("output", format!("clipboard open failed copy-only=true err=\"{e}\"")),
                }
            }) {
                log::error("output", format!("run_on_main_thread failed context=\"clipboard\" err=\"{e}\""));
            }
        }
        return;
    }

    // Auto-paste zapnuté: async task jen sleepuje, celá sekvence
    // (read previous → set → ⌘V → případný restore, jeden Clipboard handle)
    // běží v jediné main-thread closure — pořadí je tím garantované.
    let copy_clipboard = config.copy_clipboard;
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        // Async sleep — čekání blokuje jen tento task, ne executor.
        tokio::time::sleep(PASTE_DELAY).await;
        if crate::audio::is_stale(generation) {
            log::info("output", format!("paste skipped stale=true generation={generation}"));
            return;
        }
        if let Err(e) = handle.run_on_main_thread(move || {
            log::info("output", format!("paste start generation={generation}"));
            let mut clipboard = match Clipboard::new() {
                Ok(clipboard) => clipboard,
                Err(e) => {
                    log::error("output", format!("clipboard open failed context=\"paste\" err=\"{e}\""));
                    return;
                }
            };
            let previous = clipboard.get_text().ok();
            if let Err(e) = clipboard.set_text(&text) {
                log::error("output", format!("clipboard write failed context=\"paste\" err=\"{e}\""));
                return; // paste by vložil starý obsah — radši nic
            }
            log::info("output", "clipboard set ok=true context=\"paste\"");
            if let Err(e) = paste_text() {
                log::error("output", format!("paste failed err=\"{e}\""));
            } else {
                log::info("output", "paste done ok=true");
            }
            // Blokující sleep na main threadu (~250 ms): drží pořadí
            // paste → restore v rámci jednoho handle bez dalšího přehozu.
            std::thread::sleep(RESTORE_DELAY);
            if !copy_clipboard {
                if let Some(old) = previous {
                    match clipboard.set_text(&old) {
                        Ok(()) => log::info("output", "clipboard restore ok=true"),
                        Err(e) => log::error("output", format!("clipboard restore failed err=\"{e}\"")),
                    }
                }
            }
        }) {
            log::error("output", format!("run_on_main_thread failed context=\"paste\" err=\"{e}\""));
        }
    });
}

/// Simulace ⌘V: ⌘ stisknout → V kliknout → ⌘ uvolnit (API enigo 0.6).
/// Volat JEN z main threadu (CGEvent mimo main thread = segfault).
fn paste_text() -> Result<(), String> {
    // open_prompt_to_get_permissions: false — chybějící Accessibility
    // permission nesmí vyskakovat dialogem při každé dikci; jen stderr.
    let settings = Settings {
        open_prompt_to_get_permissions: false,
        ..Settings::default()
    };
    let mut enigo = Enigo::new(&settings).map_err(|e| {
        format!("input simulation unavailable (Accessibility permission?): {e}")
    })?;
    enigo.key(Key::Meta, Direction::Press).map_err(|e| e.to_string())?;
    enigo.key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| e.to_string())?;
    enigo.key(Key::Meta, Direction::Release)
        .map_err(|e| e.to_string())?;
    Ok(())
}
