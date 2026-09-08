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
        eprintln!("gettype: output skipped (cancelled)");
        return;
    }
    crate::pill::fade_out(app);

    let config = settings::load();

    // Auto-paste vypnuté: buď jen clipboard (Copy zapnuto), nebo nic.
    // Clipboard i tady patří na main thread (NSPasteboard).
    if !config.auto_paste {
        if config.copy_clipboard {
            let handle = app.clone();
            if let Err(e) = handle.run_on_main_thread(move || {
                if crate::audio::is_stale(generation) {
                    eprintln!("gettype: clipboard set skipped (cancelled)");
                    return;
                }
                match Clipboard::new() {
                    Ok(mut clipboard) => match clipboard.set_text(&text) {
                        Ok(()) => eprintln!("gettype: clipboard set ok (copy-only)"),
                        Err(e) => eprintln!("gettype: clipboard write failed: {e}"),
                    },
                    Err(e) => eprintln!("gettype: clipboard open failed: {e}"),
                }
            }) {
                eprintln!("gettype: run_on_main_thread failed (clipboard): {e}");
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
            eprintln!("gettype: paste skipped (cancelled)");
            return;
        }
        if let Err(e) = handle.run_on_main_thread(move || {
            eprintln!("gettype: paste start");
            let mut clipboard = match Clipboard::new() {
                Ok(clipboard) => clipboard,
                Err(e) => {
                    eprintln!("gettype: clipboard open failed: {e}");
                    return;
                }
            };
            let previous = clipboard.get_text().ok();
            if let Err(e) = clipboard.set_text(&text) {
                eprintln!("gettype: clipboard write failed: {e}");
                return; // paste by vložil starý obsah — radši nic
            }
            eprintln!("gettype: clipboard set ok");
            if let Err(e) = paste_text() {
                eprintln!("gettype: paste failed: {e}");
            } else {
                eprintln!("gettype: paste end");
            }
            // Blokující sleep na main threadu (~250 ms): drží pořadí
            // paste → restore v rámci jednoho handle bez dalšího přehozu.
            std::thread::sleep(RESTORE_DELAY);
            if !copy_clipboard {
                if let Some(old) = previous {
                    match clipboard.set_text(&old) {
                        Ok(()) => eprintln!("gettype: clipboard restore ok"),
                        Err(e) => eprintln!("gettype: clipboard restore failed: {e}"),
                    }
                }
            }
        }) {
            eprintln!("gettype: run_on_main_thread failed (paste): {e}");
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
