// gettype — decentní SYSTÉMOVÉ zvuky pro start/stop nahrávání.
//
// Bez nových dependencí: fire-and-forget `afplay` na bundlované
// `/System/Library/Sounds/*.aiff` (žádné vlastní audio soubory v repu).
// Start = Funk (výraznější cink), stop/cancel = Pop (hlubší cink) — krátké,
// odlišitelné. Start hraje s `-v 2` (dvojnásobek defaultu 1), aby byl
// slyšitelně výraznější, ne uřvaný (afplay bere 0–255).
//
// Best-effort: chyby jen do `crate::log`, nikdy neblokuje audio vlákno
// (vlastní thread, detached proces). Na ne-macOS je no-op.

/// Start nahrávání — výrazné krátké cinknutí, boostnutá hlasitost.
pub fn play_start() {
    play("start", "/System/Library/Sounds/Funk.aiff", Some("2"));
}

/// Stop / cancel nahrávání — hlubší krátké cinknutí, default hlasitost.
pub fn play_stop() {
    play("stop", "/System/Library/Sounds/Pop.aiff", None);
}

#[cfg(target_os = "macos")]
fn play(which: &'static str, path: &'static str, volume: Option<&'static str>) {
    std::thread::spawn(move || {
        let mut cmd = std::process::Command::new("/usr/bin/afplay");
        if let Some(v) = volume {
            cmd.arg("-v").arg(v);
        }
        match cmd.arg(path).output()
        {
            Ok(out) if out.status.success() => {}
            Ok(out) => crate::log::warn(
                "sound",
                format!(
                    "play ok=false which=\"{which}\" path=\"{path}\" status=\"{}\" stderr=\"{}\"",
                    out.status,
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ),
            Err(e) => crate::log::warn(
                "sound",
                format!("play ok=false which=\"{which}\" path=\"{path}\" err=\"{e}\""),
            ),
        }
    });
}

#[cfg(not(target_os = "macos"))]
fn play(_which: &'static str, _path: &'static str, _volume: Option<&'static str>) {}
