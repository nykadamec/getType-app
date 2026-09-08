// gettype — globální zkratka (tauri-plugin-global-shortcut).
//
// Jeden globální handler pro všechny zkratky; režim (push_to_talk/toggle)
// se čte vždy čerstvě z configu, takže změna ve settings platí okamžitě.

use tauri::plugin::TauriPlugin;
use tauri::AppHandle;
use tauri::Wry;
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState,
};

use crate::{audio, settings};

/// Plugin s globálním handlerem. Registruje se před setupem.
pub fn plugin() -> TauriPlugin<Wry> {
    ShortcutBuilder::new()
        .with_handler(|app, _shortcut, event| {
            let mode = settings::load().mode;
            let pressed = event.state == ShortcutState::Pressed;
            match mode.as_str() {
                // toggle: reagujeme jen na stisk — vnitřní stav rozhodne start/stop
                "toggle" => {
                    if pressed {
                        audio::toggle(app);
                    }
                }
                // push_to_talk (default): držení = nahrávání, puštění = stop
                _ => {
                    if pressed {
                        audio::start(app);
                    } else if audio::is_recording(app) {
                        audio::stop(app);
                    }
                }
            }
        })
        .build()
}

/// Přečte config.hotkey a (znovu) registruje globální zkratku.
/// Konflikt zkratky vrací Err stringem (frontend ho jen zaloguje).
pub fn apply(app: &AppHandle) -> Result<(), String> {
    let config = settings::load();
    let shortcut = parse_hotkey(&config.hotkey)?;
    let shortcuts = app.global_shortcut();
    shortcuts
        .unregister_all()
        .map_err(|e| format!("unregister_all failed: {e}"))?;
    shortcuts
        .register(shortcut)
        .map_err(|e| format!("hotkey \"{}\" registration failed: {e}", config.hotkey))
}

/// Parse formátu ukládaného ze settings.js: "Option+Space", "Command+Shift+K".
/// Modifikátory → ALT/META/CONTROL/SHIFT; klávesa dle event.code názvů.
fn parse_hotkey(spec: &str) -> Result<Shortcut, String> {
    let mut mods = Modifiers::empty();
    let mut key: Option<Code> = None;
    for part in spec.split('+') {
        let token = part.trim();
        if token.is_empty() {
            return Err(format!("invalid hotkey \"{spec}\" (empty part)"));
        }
        match token.to_ascii_lowercase().as_str() {
            "option" | "alt" => mods |= Modifiers::ALT,
            // Na macOS Cmd = META (eventy) i SUPER (parser pluginu) berou
            // Carbon registraci stejně jako příkazovou klávesu.
            "command" | "cmd" | "meta" => mods |= Modifiers::META,
            "control" | "ctrl" => mods |= Modifiers::CONTROL,
            "shift" => mods |= Modifiers::SHIFT,
            _ => {
                if key.is_some() {
                    return Err(format!("invalid hotkey \"{spec}\" (more than one key)"));
                }
                key = Some(parse_key(token)?);
            }
        }
    }
    let key = key.ok_or_else(|| format!("invalid hotkey \"{spec}\" (no key)"))?;
    Ok(Shortcut::new(Some(mods), key))
}

/// Klávesa: slovní názvy, F1–F24, písmena A–Z, cifry 0–9, interpunkce.
fn parse_key(token: &str) -> Result<Code, String> {
    let lower = token.to_ascii_lowercase();
    let named = match lower.as_str() {
        "space" => Code::Space,
        "enter" | "return" => Code::Enter,
        "tab" => Code::Tab,
        "backspace" => Code::Backspace,
        "escape" | "esc" => Code::Escape,
        "up" | "arrowup" => Code::ArrowUp,
        "down" | "arrowdown" => Code::ArrowDown,
        "left" | "arrowleft" => Code::ArrowLeft,
        "right" | "arrowright" => Code::ArrowRight,
        "home" => Code::Home,
        "end" => Code::End,
        "pageup" => Code::PageUp,
        "pagedown" => Code::PageDown,
        "delete" => Code::Delete,
        "capslock" => Code::CapsLock,
        // interpunkce — settings.js ukládá i samotné znaky
        "-" | "minus" => Code::Minus,
        "=" | "equal" => Code::Equal,
        "," | "comma" => Code::Comma,
        "." | "period" => Code::Period,
        "/" | "slash" => Code::Slash,
        ";" | "semicolon" => Code::Semicolon,
        "'" | "quote" => Code::Quote,
        "`" | "backquote" => Code::Backquote,
        "[" | "bracketleft" => Code::BracketLeft,
        "]" | "bracketright" => Code::BracketRight,
        "\\" | "backslash" => Code::Backslash,
        _ => {
            // F-klávesy: F1..F24
            if let Some(num) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                if (1..=24).contains(&num) {
                    return Ok(f_key(num));
                }
            }
            // jediné písmeno a–z
            let mut chars = lower.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                if let Some(code) = letter_key(c).or_else(|| digit_key(c)) {
                    return Ok(code);
                }
            }
            return Err(format!("unsupported hotkey key: \"{token}\""));
        }
    };
    Ok(named)
}

fn f_key(num: u8) -> Code {
    // F1..F24 jdou v enumu za sebou
    match num {
        1 => Code::F1,
        2 => Code::F2,
        3 => Code::F3,
        4 => Code::F4,
        5 => Code::F5,
        6 => Code::F6,
        7 => Code::F7,
        8 => Code::F8,
        9 => Code::F9,
        10 => Code::F10,
        11 => Code::F11,
        12 => Code::F12,
        13 => Code::F13,
        14 => Code::F14,
        15 => Code::F15,
        16 => Code::F16,
        17 => Code::F17,
        18 => Code::F18,
        19 => Code::F19,
        20 => Code::F20,
        21 => Code::F21,
        22 => Code::F22,
        23 => Code::F23,
        _ => Code::F24,
    }
}

fn letter_key(c: char) -> Option<Code> {
    let code = match c {
        'a' => Code::KeyA,
        'b' => Code::KeyB,
        'c' => Code::KeyC,
        'd' => Code::KeyD,
        'e' => Code::KeyE,
        'f' => Code::KeyF,
        'g' => Code::KeyG,
        'h' => Code::KeyH,
        'i' => Code::KeyI,
        'j' => Code::KeyJ,
        'k' => Code::KeyK,
        'l' => Code::KeyL,
        'm' => Code::KeyM,
        'n' => Code::KeyN,
        'o' => Code::KeyO,
        'p' => Code::KeyP,
        'q' => Code::KeyQ,
        'r' => Code::KeyR,
        's' => Code::KeyS,
        't' => Code::KeyT,
        'u' => Code::KeyU,
        'v' => Code::KeyV,
        'w' => Code::KeyW,
        'x' => Code::KeyX,
        'y' => Code::KeyY,
        'z' => Code::KeyZ,
        _ => return None,
    };
    Some(code)
}

fn digit_key(c: char) -> Option<Code> {
    let code = match c {
        '0' => Code::Digit0,
        '1' => Code::Digit1,
        '2' => Code::Digit2,
        '3' => Code::Digit3,
        '4' => Code::Digit4,
        '5' => Code::Digit5,
        '6' => Code::Digit6,
        '7' => Code::Digit7,
        '8' => Code::Digit8,
        '9' => Code::Digit9,
        _ => return None,
    };
    Some(code)
}
