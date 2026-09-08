// gettype — menu bar dictation app (MVP skeleton)

mod settings;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // No Dock icon — menu bar only (macOS)
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            use tauri::menu::{Menu, MenuItem};
            use tauri::tray::TrayIconBuilder;

            let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit gettype", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit])?;

            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
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
            .inner_size(560.0, 660.0)
            .resizable(false)
            .visible(false)
            .decorations(true)
            .build()?;

            Ok(())
        })
        .on_menu_event(|app, event| match event.id.as_ref() {
            "settings" => {
                if let Some(win) = app.get_webview_window("settings") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_window_event(|window, event| {
            // Settings window hides instead of closing — app lives in the tray.
            if window.label() == "settings" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_settings,
            save_config,
            save_api_key
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
