// gettype — menu bar dictation app (MVP skeleton)

mod audio;
mod history;
mod hotkey;
mod log;
mod output;
mod permissions;
mod pill;
mod preprocess;
mod settings;
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
            let quit = MenuItem::with_id(app, "quit", "Quit gettype", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit])?;

            // Vlastní A-Waveform tray ikona (černá na transparentní) v template
            // režimu — macOS ji přebarví podle světlého/tmavého menu baru.
            // API ověřeno proti tauri 2.11.5: TrayIconBuilder::icon_as_template
            // (src/tray/mod.rs:295), png dekódování přes image-png feature.
            let tray_icon =
                tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))?;

            TrayIconBuilder::with_id("main-tray")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("gettype")
                .menu(&menu)
                .show_menu_on_left_click(true)
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

            // Onboarding okno (první spuštění): skryté, ukáže se jen když
            // chybí config.json. Frontend dodá paralelně @designer
            // (src/onboarding.*) — backend jen okno + commandy.
            WebviewWindowBuilder::new(
                app,
                "onboarding",
                WebviewUrl::App("onboarding.html".into()),
            )
            .title("Welcome to gettype")
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
                if let Some(win) = app.get_webview_window("settings") {
                    // Dock ikona jen když je Settings viditelné (plán-dock-settings).
                    #[cfg(target_os = "macos")]
                    let _ = app.set_activation_policy(tauri::ActivationPolicy::Regular);
                    let _ = win.show();
                    let _ = win.set_focus();
                }
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
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_config,
            save_api_key,
            verify::verify_api_key,
            set_launch_at_login,
            get_launch_at_login,
            apply_hotkey,
            permissions::get_mic_permission,
            permissions::request_mic_permission,
            permissions::get_accessibility_permission,
            permissions::open_accessibility_settings,
            finish_onboarding,
            history::list_history,
            history::copy_history_entry,
            history::delete_history_entry,
            history::clear_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
