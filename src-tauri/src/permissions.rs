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
        Ok(current_mic_status())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok("unknown".to_string())
    }
}

/// Aktuální stav mikrofonu ve slovníku get_mic_permission.
/// Sdílí request_mic_permission (čte stav znovu po doběhnutí promptu).
#[cfg(target_os = "macos")]
fn current_mic_status() -> String {
    use objc2_av_foundation::{
        AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio,
    };
    // Statika AVMediaTypeAudio je `Option` — None jen pokud by
    // framework konstantu neexportoval (prakticky nemožné).
    let status = unsafe {
        match AVMediaTypeAudio {
            Some(audio) => AVCaptureDevice::authorizationStatusForMediaType(audio),
            None => return "unknown".to_string(),
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
    s.to_string()
}

/// Vyvolá systémový prompt mikrofonu (`requestAccessForMediaType`)
/// a vrátí výsledný stav ve slovníku get_mic_permission.
/// Async command (neblokuje Tauri IPC vlákno): pokud už bylo rozhodnuto,
/// vrátí stav hned bez promptu. Jinak čeká na completion-handler
/// (Send-safe `StackBlock`, výsledek přes `oneshot`) s timeoutem 60 s —
/// při timeoutu vrací `Err("timeout")`, UI tak nikdy nezamrzne na „Waiting…".
#[tauri::command]
pub async fn request_mic_permission() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        // Rychlá cesta: bez předchozího rozhodnutí by prompt bez
        // NSMicrophoneUsageDescription v bundlu vůbec nevyskočil.
        if current_mic_status() != "not-determined" {
            let status = current_mic_status();
            crate::log::info("permissions", format!("mic request skipped already-decided=true status=\"{status}\""));
            return Ok(status);
        }
        use objc2::runtime::Bool;
        use objc2_av_foundation::{AVCaptureDevice, AVMediaTypeAudio};

        // None jen pokud by framework konstantu neexportoval (viz výše).
        let audio = unsafe { AVMediaTypeAudio };
        let Some(audio) = audio else {
            return Ok("unknown".to_string());
        };
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        {
            // StackBlock vyžaduje Clone closure — sender proto v
            // Arc<Mutex<Option<..>>>, předá se právě jednou.
            let tx = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
            let tx_cb = tx.clone();
            let block = block2::StackBlock::new(move |_granted: Bool| {
                if let Ok(mut guard) = tx_cb.lock() {
                    if let Some(tx) = guard.take() {
                        let _ = tx.send(());
                    }
                }
            });
            unsafe {
                AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &block);
            }
            // AVFoundation si handler zkopíruje na heap (Block_copy, proto
            // StackBlock, ne RcBlock — completion běží na libovolné
            // frontě/jiném threadu), takže náš stack blok smí zaniknout
            // hned po návratu volání. Zároveň tím blok nevisí přes `await`
            // — async command Tauri vyžaduje Send future.
        }
        match tokio::time::timeout(std::time::Duration::from_secs(60), rx).await {
            Ok(Ok(())) => {
                let status = current_mic_status();
                crate::log::info("permissions", format!("mic request done status=\"{status}\""));
                Ok(status)
            }
            Ok(Err(e)) => {
                crate::log::error("permissions", format!("mic request failed err=\"{e}\""));
                Err(e.to_string())
            }
            Err(_) => {
                crate::log::warn("permissions", "mic request timeout after=60s");
                Err("timeout".to_string())
            }
        }
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

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrustedWithOptions(options: *const std::ffi::c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: *const std::ffi::c_void;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFBooleanTrue: *const std::ffi::c_void;
    static kCFTypeDictionaryKeyCallBacks: std::ffi::c_void;
    static kCFTypeDictionaryValueCallBacks: std::ffi::c_void;
    fn CFDictionaryCreate(
        allocator: *const std::ffi::c_void,
        keys: *const *const std::ffi::c_void,
        values: *const *const std::ffi::c_void,
        num_values: i64,
        key_callbacks: *const std::ffi::c_void,
        value_callbacks: *const std::ffi::c_void,
    ) -> *const std::ffi::c_void;
    fn CFRelease(cf: *const std::ffi::c_void);
}

/// Otevře systémový dialog vedoucí do System Settings
/// (`AXIsProcessTrustedWithOptions` s `kAXTrustedCheckOptionPrompt=true`).
/// Čisté FFI jako `AXIsProcessTrusted` výše, žádný nový crate.
#[tauri::command]
pub fn open_accessibility_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use std::ptr;
        unsafe {
            let key = kAXTrustedCheckOptionPrompt;
            let value = kCFBooleanTrue;
            let options = CFDictionaryCreate(
                ptr::null(),
                &key,
                &value,
                1,
                ptr::addr_of!(kCFTypeDictionaryKeyCallBacks) as *const std::ffi::c_void,
                ptr::addr_of!(kCFTypeDictionaryValueCallBacks) as *const std::ffi::c_void,
            );
            if options.is_null() {
                return Err("failed to create options dictionary".to_string());
            }
            AXIsProcessTrustedWithOptions(options);
            CFRelease(options);
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}
