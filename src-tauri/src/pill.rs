// gettype — pilulka: CSS fade in/out řízený eventy místo okamžitého hide().
//
// fade_out() emituje `pill-fade-out` (frontend přepne body na `hiding`
// → opacity/transform přechod) a teprve po jeho délce zavolá window.hide().
// Generační číslo dělá helper idempotentním (dvojí fade skryje jen jednou)
// a chrání proti závodu „pozdní hide přepíše čerstvé show“ při rychlém
// restartu nahrávání.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::log;

/// Délka CSS fade-outu (body.hiding, 220 ms) + malá rezerva před hide().
const FADE_MS: u64 = 260;

/// Generační číslo: každý fade/show ho zvyšuje. Naplánovaný hide se provede
/// jen tehdy, pokud se generace mezitím nezměnila.
static FADE_GEN: AtomicU64 = AtomicU64::new(0);

/// Fade out pilulky: event do frontendu, pak (po `FADE_MS`) `window.hide()`.
/// Idempotentní — opakované volání během jednoho fade nespustí druhý hide.
pub fn fade_out(app: &AppHandle) {
    let generation = FADE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    log::info("pill", format!("fade_out scheduled generation={generation}"));
    let _ = app.emit("pill-fade-out", serde_json::json!({}));

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(FADE_MS)).await;
        // Mezitím začal nový fade nebo se pilulka znovu ukázala → nechám
        // rozhodnout novější akci, žádný dvojitý/pozdní hide.
        if FADE_GEN.load(Ordering::SeqCst) != generation {
            log::info("pill", format!("hide suppressed stale=true generation={generation}"));
            return;
        }
        // window.hide() patří na main thread — sjednoceno s paste cestou.
        let inner = handle.clone();
        if let Err(e) = handle.run_on_main_thread(move || {
            if let Some(pill) = inner.get_webview_window("pill") {
                let _ = pill.hide();
                log::info("pill", format!("hide done generation={generation}"));
            }
        }) {
            log::error("pill", format!("run_on_main_thread failed context=\"hide\" err=\"{e}\""));
        }
    });
}

/// Ukáže pilulku (bez focusu — řeší builder) a zneplatní čekající fade-hide,
/// aby rychlé nové nahrávání nebylo „dodoběno“ zpožděným skrytím.
pub fn show(app: &AppHandle) {
    FADE_GEN.fetch_add(1, Ordering::SeqCst);
    log::info("pill", "show");
    if let Some(pill) = app.get_webview_window("pill") {
        let _ = pill.show();
    }
}
