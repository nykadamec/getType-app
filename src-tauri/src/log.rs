// gettype — jednotný strukturovaný log (jen stderr, žádné závislosti).
//
// Formát: `[01:23:45.678] [audio] start ok=true device="..." rate=16000`
// WARN/ERROR přidávají značku za event: `[..] [audio] WARN ...`.
// Čas je UTC time-of-day ze SystemTime (bez chrono/time crate).
// Pouze logování — žádná business logika, žádné paniky.

fn timestamp() -> String {
    let ms_since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let ms_of_day = ms_since_epoch % 86_400_000;
    let hh = ms_of_day / 3_600_000;
    let mm = (ms_of_day % 3_600_000) / 60_000;
    let ss = (ms_of_day % 60_000) / 1000;
    let ms = ms_of_day % 1000;
    format!("{hh:02}:{mm:02}:{ss:02}.{ms:03}")
}

/// Info řádek: `[čas] [event] detail`.
pub fn info(event: &str, detail: impl AsRef<str>) {
    eprintln!("[{}] [{}] {}", timestamp(), event, detail.as_ref());
}

/// Varování: `[čas] [event] WARN detail`.
pub fn warn(event: &str, detail: impl AsRef<str>) {
    eprintln!("[{}] [{}] WARN {}", timestamp(), event, detail.as_ref());
}

/// Chyba: `[čas] [event] ERROR detail`.
pub fn error(event: &str, detail: impl AsRef<str>) {
    eprintln!("[{}] [{}] ERROR {}", timestamp(), event, detail.as_ref());
}
