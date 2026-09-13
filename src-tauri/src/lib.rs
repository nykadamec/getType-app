// gettype — menu bar dictation app (MVP skeleton)

mod audio;
mod history;
mod hotkey;
mod llm;
mod log;
mod output;
mod permissions;
mod pill;
mod preprocess;
mod settings;
mod sound;
mod stt;
mod verify;

use std::sync::Mutex;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
fn load_settings() -> settings::LoadedSettings {
    let config = settings::load();
    // Keychain errors are non-fatal — surface as "no key".
    let has_api_key = settings::get_api_key()
        .map(|key| key.is_some())
        .unwrap_or(false);
    settings::LoadedSettings {
        config,
        has_api_key,
    }
}

#[tauri::command]
fn save_config(config: settings::Config) -> Result<(), String> {
    settings::save(&config)
}

/// Re-registers the global hotkey from the current config.
/// Conflict (shortcut taken by another app) comes back as Err.
#[tauri::command]
fn apply_hotkey(app: tauri::AppHandle) -> Result<(), String> {
    hotkey::apply(&app)
}

/// Empty string removes the key from the keychain.
#[tauri::command]
fn save_api_key(key: String) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        settings::delete_api_key()
    } else {
        settings::set_api_key(key)
    }
}

/// Zapne/vypne spouštění po přihlášení (macOS LaunchAgent).
#[tauri::command]
fn set_launch_at_login(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

/// Aktuální stav spouštění po přihlášení.
#[tauri::command]
fn get_launch_at_login(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Ukaže okno Settings (menu-bar-only → Dock ikona, viz plán-dock-settings).
/// Sdílí logiku tray menu handleru níže — volá i ozubené kolo v popoveru.
fn show_settings_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("settings") {
        // Dock ikona jen když je Settings viditelné (plán-dock-settings).
        #[cfg(target_os = "macos")]
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
        let _ = win.show();
        let _ = win.set_focus();
    }
}

/// Ozubené kolo v tray popoveru → Settings. Popover se zavře,
/// Settings přebírá popředí.
#[tauri::command]
fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    crate::log::info("tray", "open_settings called");
    show_settings_window(&app);
    if let Some(pop) = app.get_webview_window("popover") {
        match pop.hide() {
            Ok(()) => crate::log::info("tray", "open_settings popover hide ok=true"),
            Err(e) => crate::log::warn("tray", format!("open_settings popover hide ok=false err=\"{e}\"")),
        }
    } else {
        crate::log::info("tray", "open_settings popover missing=false");
    }
    Ok(())
}

/// Most pro frontend logy (popover instrumentace dvojitého bliknutí).
/// Jen přeposílá do stderr logu — žádná business logika, žádná změna chování.
/// Kryje stávající `core:default` (stejně jako ostatní commandy popoveru),
/// žádná nová capability není potřeba.
#[tauri::command]
fn log_frontend(level: String, msg: String) {
    match level.as_str() {
        "warn" => crate::log::warn("popover", msg),
        "error" => crate::log::error("popover", msg),
        _ => crate::log::info("popover", msg),
    }
}

/// Skryje tray popover (Escape ve frontendu, rezerva k blur handleru níže).
#[tauri::command]
fn hide_popover(app: tauri::AppHandle) -> Result<(), String> {
    crate::log::info("tray", "hide_popover called");
    if let Some(pop) = app.get_webview_window("popover") {
        match pop.hide() {
            Ok(()) => crate::log::info("tray", "hide_popover hide ok=true"),
            Err(e) => crate::log::warn("tray", format!("hide_popover hide ok=false err=\"{e}\"")),
        }
    } else {
        crate::log::info("tray", "hide_popover popover missing=false");
    }
    Ok(())
}

/// Aktuální stav rekordéru pro cold-start sync pilulky (`pill.js` si ho
/// vyžádá po attachi listenerů — zachytí `recording-started` emitnutý dřív,
/// než studené webview stihlo subscribnout eventy, viz fadeIn race).
/// "transcribing" | "recording" | "idle".
#[tauri::command]
fn get_recorder_state(app: tauri::AppHandle) -> String {
    if audio::is_transcribing() {
        "transcribing".to_string()
    } else if audio::is_recording(&app) {
        "recording".to_string()
    } else {
        "idle".to_string()
    }
}

/// Šířka popover okna — musí sedět s `inner_size` v setup() a CSS
/// (karta 300 + 18 px padding po stranách).
const POPOVER_W: f64 = 336.0;

/// Přepínač popoveru: viditelný → skrýt, skrytý → ukotvit pod tray
/// ikonu a ukázat + focus (bez focusu by blur-zavírání nefungovalo).
fn toggle_popover(app: &tauri::AppHandle, rect: Option<tauri::Rect>) {
    crate::log::info("tray", format!("toggle called rect={rect:?}"));
    let Some(win) = app.get_webview_window("popover") else {
        crate::log::warn("tray", "toggle popover missing=false");
        return;
    };
    let visible = win.is_visible().unwrap_or(false);
    crate::log::info("tray", format!("toggle visible={visible}"));
    if visible {
        crate::log::info("tray", "toggle decision=hide reason=visible");
        match win.hide() {
            Ok(()) => crate::log::info("tray", "toggle hide ok=true"),
            Err(e) => crate::log::warn("tray", format!("toggle hide ok=false err=\"{e}\"")),
        }
        return;
    }
    crate::log::info("tray", "toggle decision=show reason=hidden");
    let t0 = std::time::Instant::now();
    position_popover(app, &win, rect);
    crate::log::info(
        "tray",
        format!("toggle about-to-show elapsed_ms={}", t0.elapsed().as_millis()),
    );
    match win.show() {
        Ok(()) => crate::log::info(
            "tray",
            format!("toggle show ok=true elapsed_ms={}", t0.elapsed().as_millis()),
        ),
        Err(e) => crate::log::warn(
            "tray",
            format!(
                "toggle show ok=false err=\"{e}\" elapsed_ms={}",
                t0.elapsed().as_millis()
            ),
        ),
    }
    match win.set_focus() {
        Ok(()) => crate::log::info(
            "tray",
            format!(
                "toggle focus ok=true elapsed_ms={}",
                t0.elapsed().as_millis()
            ),
        ),
        Err(e) => crate::log::warn(
            "tray",
            format!(
                "toggle focus ok=false err=\"{e}\" elapsed_ms={}",
                t0.elapsed().as_millis()
            ),
        ),
    }
}

/// Ukotvení popoveru POD tray ikonu, vodorovně centrované na její střed.
///
/// Tauri 2.11 `TrayIconEvent::Click` nese `rect` = pozici a velikost tray
/// ikony ve FYZICKÝCH px (tauri::tray::TrayIconEvent) — přesnější než odhad
/// z rohu obrazovky. Když je rect nulový (platforma ho nedodá), padáme
/// zpět na pravý horní roh primárního monitoru pod menu bar.
/// Fyzické px dělíme scale factorem monitoru → logické body pro set_position.
fn position_popover(app: &tauri::AppHandle, win: &tauri::WebviewWindow, rect: Option<tauri::Rect>) {
    const GAP: f64 = 6.0; // mezera mezi menu barem / ikonou a kartou
    const EDGE: f64 = 8.0; // min. odstup karty od stran obrazovky
    let t0 = std::time::Instant::now();
    crate::log::info("tray", format!("position rect={rect:?}"));

    let (mon_x, mon_y, mon_w) = match app.primary_monitor().ok().flatten() {
        Some(monitor) => {
            let scale = monitor.scale_factor();
            let pos = monitor.position();
            let size = monitor.size();
            (
                pos.x as f64 / scale,
                pos.y as f64 / scale,
                size.width as f64 / scale,
            )
        }
        None => (0.0, 0.0, 1440.0), // fallback bez monitoru
    };

    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(2.0);

    // 1) vodorovně na střed ikony (nebo roh monitoru), 2) clamp na obrazovku.
    // tauri::Rect nese dpi enumy Position/Size → nejdřív do fyzických px,
    // pak dělíme scale factorem na logické body.
    let (mut x, y) = match rect {
        Some(r) => {
            let pos: tauri::PhysicalPosition<f64> = r.position.to_physical(scale);
            let size: tauri::PhysicalSize<f64> = r.size.to_physical(scale);
            crate::log::info(
                "tray",
                format!(
                    "position icon x={:.1} y={:.1} w={:.1} h={:.1} scale={scale}",
                    pos.x, pos.y, size.width, size.height
                ),
            );
            if size.width > 0.0 {
                let center = (pos.x + size.width / 2.0) / scale;
                let bottom = (pos.y + size.height) / scale;
                let (px, py) = (center - POPOVER_W / 2.0, bottom + GAP);
                crate::log::info("tray", format!("position anchor center={center:.1} bottom={bottom:.1} x={px:.1} y={py:.1}"));
                (px, py)
            } else {
                crate::log::warn("tray", "position rect zero fallback=corner");
                (mon_x + mon_w - POPOVER_W - EDGE, mon_y + 30.0)
            }
        }
        None => {
            crate::log::warn("tray", "position rect none fallback=corner");
            (mon_x + mon_w - POPOVER_W - EDGE, mon_y + 30.0)
        }
    };
    let lo = mon_x + EDGE;
    let hi = (mon_x + mon_w - POPOVER_W - EDGE).max(mon_x + EDGE);
    let x_raw = x;
    x = x.clamp(lo, hi);

    crate::log::info(
        "tray",
        format!("position result x={x:.1} y={y:.1} raw_x={x_raw:.1} clamp=[{lo:.1},{hi:.1}] mon_x={mon_x:.1} mon_w={mon_w:.1} elapsed_ms={}", t0.elapsed().as_millis()),
    );
    match win.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y })) {
        Ok(()) => crate::log::info("tray", format!("position set ok=true x={x:.1} y={y:.1} elapsed_ms={}", t0.elapsed().as_millis())),
        Err(e) => crate::log::warn("tray", format!("position set ok=false err=\"{e}\" elapsed_ms={}", t0.elapsed().as_millis())),
    }
}

/// Konec onboardingu: uloží aktuální config (vznikne config.json =
/// značka hotova), skryje okno `onboarding` a vrátí menu-bar-only režim.
/// Stejný vzor jako hide handler Settings níže.
#[tauri::command]
fn finish_onboarding(app: tauri::AppHandle) -> Result<(), String> {
    let config = settings::load();
    settings::save(&config)?;
    crate::log::info("onboarding", "finish ok=true");
    if let Some(win) = app.get_webview_window("onboarding") {
        let _ = win.hide();
    }
    #[cfg(target_os = "macos")]
    let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(hotkey::plugin())
        .setup(|app| {
            // No Dock icon — menu bar only (macOS)
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;

            let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit getType", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit])?;

            // Vlastní A-Waveform tray ikona (černá na transparentní) v template
            // režimu — macOS ji přebarví podle světlého/tmavého menu baru.
            // API ověřeno proti tauri 2.11.5: TrayIconBuilder::icon_as_template
            // (src/tray/mod.rs:295), png dekódování přes image-png feature.
            let tray_icon =
                tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;

            crate::log::info(
                "tray",
                "register mode=popover show_menu_on_left_click=false menu=settings,quit",
            );
            // Menu TRVALE NEPŘIPOJujeme (.menu vynecháno): na macOS si
            // připojené nativní menu (NSStatusItem.setMenu) bere levý klik
            // pro sebe a Click{Left,Up} vůbec nedorazí do on_tray_icon_event.
            // Menu držíme stranou a ukážeme ho programově jen na pravý klik.
            let context_menu = menu.clone();
            TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("getType")
                // Levý klik = tray popover (viz on_tray_icon_event níže),
                // pravý klik = programové show_menu (set_menu + inner show).
                .show_menu_on_left_click(false)
                .on_tray_icon_event(move |tray, event| {
                    use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
                    // Toggle až na puštění tlačítka (Up), ne na stisk —
                    // jinak by Down + Up překlopily popover dvakrát.
                    if let TrayIconEvent::Click {
                        button,
                        button_state,
                        rect,
                        ..
                    } = event
                    {
                        let scale = tray
                            .app_handle()
                            .primary_monitor()
                            .ok()
                            .flatten()
                            .map(|m| m.scale_factor())
                            .unwrap_or(2.0);
                        let pos: tauri::PhysicalPosition<f64> = rect.position.to_physical(scale);
                        let size: tauri::PhysicalSize<f64> = rect.size.to_physical(scale);
                        crate::log::info(
                            "tray",
                            format!(
                                "click button={button:?} state={button_state:?} x={:.1} y={:.1} w={:.1} h={:.1}",
                                pos.x, pos.y, size.width, size.height
                            ),
                        );
                        if size.width <= 0.0 {
                            crate::log::warn("tray", "click rect zero fallback=corner");
                        }
                        if button == MouseButton::Left && button_state == MouseButtonState::Up {
                            toggle_popover(tray.app_handle(), Some(rect));
                        } else if button == MouseButton::Right
                            && button_state == MouseButtonState::Up
                        {
                            crate::log::info("tray", "menu show reason=right-click");
                            if let Err(e) = tray.set_menu(Some(context_menu.clone())) {
                                crate::log::warn(
                                    "tray",
                                    format!("menu attach ok=false err=\"{e}\""),
                                );
                            }
                            match tray.with_inner_tray_icon(|inner| inner.show_menu()) {
                                Ok(()) => crate::log::info("tray", "menu show ok=true"),
                                Err(e) => crate::log::warn(
                                    "tray",
                                    format!("menu show ok=false err=\"{e}\""),
                                ),
                            }
                            // Odpojit menu hned po zavření: show_menu je synchronní
                            // performClick (vrátí se až po zavření menu). Bez detach
                            // by si nativní NSStatusItem bral i levé kliky a popover
                            // by se už neotevřel. Levý klik má přednost — chybu
                            // jen zalogovat a pokračovat.
                            match tray.set_menu(None::<tauri::menu::Menu<tauri::Wry>>) {
                                Ok(()) => crate::log::info("tray", "menu detach ok=true"),
                                Err(e) => crate::log::warn(
                                    "tray",
                                    format!("menu detach ok=false err=\"{e}\""),
                                ),
                            }
                        } else {
                            crate::log::info("tray", "click ignored reason=not-left-up");
                        }
                    } else {
                        crate::log::info("tray", format!("event ignored kind={event:?}"));
                    }
                })
                .build(app)?;

            // Hidden settings window — revealed from the tray menu.
            WebviewWindowBuilder::new(
                app,
                "settings",
                WebviewUrl::App("settings.html".into()),
            )
            .title("Settings")
            .inner_size(780.0, 580.0)
            .resizable(false)
            .visible(false)
            .decorations(true)
            // macOS „hidden title“ styl (jako System Settings): nativní
            // traffic lights zůstávají nad sidebarem, vlastní elipsy
            // nekreslíme (varianta B — čistší výsledek, viz report).
            // Sidebar drží 28px drag strip, aby světla měla kam plavat.
            // macOS „hidden title“ styl (jako System Settings): nativní
            // traffic lights zůstávají, nativní titulek je skrytý a obsah
            // se táhne až k vršku — vlastní strip řeší settings.html/css.
            // API ověřeno proti tauri 2.11.5: TitleBarStyle je re-export
            // tauri_utils (crate::TitleBarStyle), hidden_title(bool) na
            // WebviewWindowBuilder (macOS only). Overlay (ne Transparent):
            // jen Overlay dává fullsize_content_view(true), takže webview
            // sahá až k vršku a semafory plavou nad naším stripem na jedné
            // lince s titulkem. Transparent nechává toolbaru vlastní pruh
            // (viz tauri-runtime-wry title_bar_style) — vznikla by dvojitá
            // hlavička. Bílý NSWindow background níže řeší prosvítání.
            .title_bar_style(tauri::TitleBarStyle::Overlay)
            .hidden_title(true)
            .build()
            .map(|win| {
                // Bílý background NSWindow (Tauri docs „Transparent Titlebar
                // with Custom Window Background Color"): transparentní toolbar
                // ukazuje barvu NSWindow, ne webview — bez toho je v dark
                // mode nad bílým stripem tmavý pruh.
                #[cfg(target_os = "macos")]
                {
                    use objc2_app_kit::{NSColor, NSWindow, NSWindowButton};
                    if let Ok(ptr) = win.ns_window() {
                        unsafe {
                            let ns_window = &*(ptr as *mut NSWindow);
                            let white = NSColor::whiteColor();
                            ns_window.setBackgroundColor(Some(&white));
                            // Non-resizable okno → macOS zoom tlačítko šedí
                            // (disabled). Radši ho schováme úplně.
                            if let Some(zoom) =
                                ns_window.standardWindowButton(NSWindowButton::ZoomButton)
                            {
                                zoom.setHidden(true);
                            }
                        }
                    }
                }
                win
            })?;

            // Recording pill: 128×40, vodorovně centrovaná, ~56 px od vršku.
            // Monitor dává fyzické px → vydělíme scale factor (Retina 2×).
            let (pill_x, pill_y) = match app.primary_monitor().ok().flatten() {
                Some(monitor) => {
                    let scale = monitor.scale_factor();
                    let mx = monitor.position().x as f64;
                    let mw = monitor.size().width as f64;
                    ((mx + mw / 2.0) / scale - 64.0, 56.0)
                }
                None => ((1280.0 / 2.0) - 64.0, 56.0), // fallback bez monitoru
            };
            let pill_builder =
                WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill.html".into()))
                    .title("Recording")
                    .inner_size(128.0, 40.0)
                    .position(pill_x, pill_y)
                    .decorations(false)
                    .transparent(true) // vyžaduje macOSPrivateApi v tauri.conf.json
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .visible(false)
                    .resizable(false)
                    .focused(false) // pilulka nesmí krást focus
                    .shadow(false); // stín řeší CSS pilulky
            // Dev: obejití webview cache (macOS = nonPersistent DataStore),
            // aby se pill.html načítalo vždy čerstvé. Release beze změny.
            #[cfg(debug_assertions)]
            let pill_builder = pill_builder.incognito(true);
            pill_builder.build()?;

            // Tray popover (návrh 01 Popover — Idle, rám ncumZ v gettype.pen):
            // 336×308, bez dekorací, průhledný (stín kreslí CSS karty),
            // always-on-top, skrytý. Focusable schválně — kliknutí mimo
            // (blur) popover zavírá, viz on_window_event níže. Pozice se
            // dopočítá při každém otevření z rect tray ikony.
            let popover_builder =
                WebviewWindowBuilder::new(app, "popover", WebviewUrl::App("popover.html".into()))
                    .title("getType")
                    .inner_size(POPOVER_W, 308.0)
                    .decorations(false)
                    .transparent(true) // vyžaduje macOSPrivateApi v tauri.conf.json
                    .always_on_top(true)
                    .skip_taskbar(true)
                    .visible(false)
                    .resizable(false)
                    .focused(true)
                    .shadow(false); // stín řeší CSS karty
            // Dev: obejití webview cache (macOS = nonPersistent DataStore),
            // aby se popover.html načítalo vždy čerstvé. Release beze změny.
            #[cfg(debug_assertions)]
            let popover_builder = popover_builder.incognito(true);
            popover_builder.build()?;

            // Onboarding okno (první spuštění): skryté, ukáže se jen když
            // chybí config.json. Frontend dodá paralelně @designer
            // (src/onboarding.*) — backend jen okno + commandy.
            WebviewWindowBuilder::new(
                app,
                "onboarding",
                WebviewUrl::App("onboarding.html".into()),
            )
            .title("Welcome to getType")
            .inner_size(480.0, 640.0)
            .resizable(false)
            .visible(false)
            .build()?;

            // Stav nahrávání — drží cpal Stream živý mezi start/stop.
            app.manage(Mutex::new(audio::Recorder::default()));
            // Historie přepisů — načte history.json (chybějící/corrupt → prázdná).
            app.manage(Mutex::new(history::History::load()));

            // Globální hotkey po vytvoření oken; konflikt (obsazená zkratka)
            // jen zalogujeme, aplikace nespadne.
            if let Err(e) = hotkey::apply(app.handle()) {
                crate::log::error("hotkey", format!("apply failed err=\"{e}\""));
            } else {
                crate::log::info("hotkey", "apply ok=true");
            }

            // První spuštění = chybí config.json: Dock ikona + onboarding.
            // Stejný vzor jako tray settings handler výše. Jinak beze změny
            // (menu-bar-only). Křížek okno jen skryje — bez configu se po
            // restartu ukáže znovu.
            let first_run = settings::config_path().map(|p| !p.exists()).unwrap_or(true);
            if first_run {
                if let Some(win) = app.get_webview_window("onboarding") {
                    #[cfg(target_os = "macos")]
                    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }

            Ok(())
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                show_settings_window(app);
            }
            "quit" => {
                // Čistota: pokud běží nahrávání, stopneme (WAV se korektně zapíše
                // a stav se resetuje) a pak teprve vypneme.
                audio::stop(app);
                app.exit(0);
            }
            _ => {}
        })
        .on_window_event(|window, event| {
            // Settings i onboarding se místo zavření jen skryjí —
            // aplikace žije v trayi.
            if window.label() == "settings" || window.label() == "onboarding" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                    // Zpět do menu-bar-only režimu (plán-dock-settings).
                    #[cfg(target_os = "macos")]
                    let _ = window
                        .app_handle()
                        .set_activation_policy(tauri::ActivationPolicy::Accessory);
                }
            }
            // Klik mimo popover ho zavře (okno je focusable, viz builder).
            // Hide při ztrátě focusu je idempotentní — explicitní hide
            // (toggle, open_settings, Escape) ničemu nevadí.
            if window.label() == "popover" {
                if let tauri::WindowEvent::Focused(false) = event {
                    crate::log::info("tray", "blur hide reason=focused-false");
                    match window.hide() {
                        Ok(()) => crate::log::info("tray", "blur hide ok=true"),
                        Err(e) => crate::log::warn("tray", format!("blur hide ok=false err=\"{e}\"")),
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_config,
            save_api_key,
            settings::reveal_api_key,
            settings::masked_api_key,
            verify::verify_api_key,
            verify::test_saved_key,
            set_launch_at_login,
            get_launch_at_login,
            apply_hotkey,
            permissions::get_mic_permission,
            permissions::request_mic_permission,
            permissions::get_accessibility_permission,
            permissions::open_accessibility_settings,
            finish_onboarding,
            open_settings,
            hide_popover,
            log_frontend,
            get_recorder_state,
            history::list_history,
            history::copy_history_entry,
            history::delete_history_entry,
            history::clear_history,
            llm::ai_preview
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
