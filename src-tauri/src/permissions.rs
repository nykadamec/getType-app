// Stav systémových oprávnění (macOS) — read-only, bez vyvolání promptů.
// Oba commandy jsou synchronní a levné, invoke ze settings okna kryje
// stávající `core:default` (žádná nová capability není potřeba).

/// Stav oprávnění mikrofonu dle AVFoundation:
/// `+[AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio]`.
/// Vrací jeden z `"granted" | "denied" | "not-determined" | "unknown"`.
/// `Restricted` (rodičovská kontrola / MDM) mapujeme na `"denied"` —
/// z pohledu aplikace je hardware stejně nedostupný.
#[tauri::command]
pub fn get_mic_permission() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_av_foundation::{
            AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio,
        };
        // Statika AVMediaTypeAudio je `Option` — None jen pokud by
        // framework konstantu neexportoval (prakticky nemožné).
        let status = unsafe {
            match AVMediaTypeAudio {
                Some(audio) => AVCaptureDevice::authorizationStatusForMediaType(audio),
                None => return Ok("unknown".to_string()),
            }
        };
        let s = if status == AVAuthorizationStatus::Authorized {
            "granted"
        } else if status == AVAuthorizationStatus::Denied
            || status == AVAuthorizationStatus::Restricted
        {
            "denied"
        } else if status == AVAuthorizationStatus::NotDetermined {
            "not-determined"
        } else {
            "unknown"
        };
        Ok(s.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok("unknown".to_string())
    }
}

// AXIsProcessTrusted z ApplicationServices — přímé FFI bez nového
// crate (nejlehčí cesta, žádná závislost navíc). Vrací non-zero = trusted.
#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

/// Stav oprávnění Accessibility: `AXIsProcessTrusted()`, true = granted.
/// Neotevírá žádný dialog, jen čte aktuální stav.
#[tauri::command]
pub fn get_accessibility_permission() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        // Bezpečné: funkce nemá žádné preconditions, jen čte stav procesu.
        let trusted = unsafe { AXIsProcessTrusted() };
        Ok(trusted)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}
