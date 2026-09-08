# Escape-cancel — plán

## Cíl
Zmáčknutí Escape během nahrávání (případně transkripce) vše zruší — nic se neuloží do schránky, nic se nevloží přes ⌘V, nic se nezapíše do historie.

## Současný stav (z mapování)
- Stav: `RecorderState { Idle, Recording }` v `src-tauri/src/audio.rs:22-42`, transcribing/paste jen eventy.
- Start/stop: `hotkey.rs:16-39` → `audio::start/stop/toggle`, `Mutex<Recorder>` v `lib.rs:188` drží cpal Stream.
- Stop: `audio.rs:146-214` → WAV do `~/Library/Application Support/gettype/recordings/rec-*.wav` → `stt::transcribe`.
- STT: `stt.rs:32-56` fire-and-forget async, bez abort handle.
- Output: `output.rs:32-100` clipboard + enigo ⌘V + restore.
- Pill: `focused(false)` (`lib.rs:183`), bez key listeneru (`pill.js:114-129`) — Escape dnes nemá kam přijít.
- Cancel/Escape v runtime cestě neexistuje. Parser `hotkey.rs:92` umí `Escape`, ale `apply()` (`hotkey.rs:43-53`) registruje vždy jen 1 shortcut (unregister_all + register).
- `settings.js:197-198` Escape ruší jen hotkey-capture v Settings, nesouvisí.

## Návrh řešení
1. `audio::cancel()` — drop stream + samples bez WAV write, state → Idle, smazat rozpracovaný buffer, `pill::fade_out()` + event `recording-cancelled`.
2. Registrace globálního Escape vedle uživatelské zkratky — `hotkey.rs::apply()` musí registrovat obě (dnes `unregister_all`), handler: Escape → cancel, nikdy start/stop.
3. Ošetřit PTT: po cancelu nesmí doběhnuvší `Released` původní zkratky spustit `stop()` + transkripci (guard flag / kontrola stavu).
4. Transkripce: pokud se ruší během `transcribing` — ignorovat dobíhající výsledek (cancellation flag) nebo abort tasku; bez toho by doběhlá transkripce zapsala do schránky.
5. Output guard: `output::apply` po cancelu nic nezapisuje (pojištění přes flag/generaci).
6. Pill: po cancelu `fade_out`, timer stop (event `recording-cancelled` v `pill.js`).
7. Nahrávka na disku: při cancelu žádný WAV nevznikne (buffer se zahodí před write); případný rozpracovaný soubor smazat.

## Varianty k rozhodnutí (nevybírat sám)
- A: Escape ruší jen nahrávání, nebo i běžící transkripci?
- B: Globální holý Escape (riziko kolize s aplikacemi), nebo Escape aktivní jen když probíhá nahrávání/transkripce?
- C: Po cancelu skrýt pill okamžitě, nebo krátký „cancelled" stav?
- D: Krátká nahrávka (<300 ms) dnes padá do error — má Escape mít přednost i tam?

## Rozhodnutí uživatele (2026-09-09)
- Rozsah: zrušit celou poslední akci — nahrávání i doběhlou transkripci, nic do schránky/paste/historie.
- Aktivace: Escape reaguje jen když probíhá akce (recording/transcribing), mimo ni nic nedělá.
- Pill: po zrušení skrýt hned, žádný „cancelled" stav.
- Stav: čeká na schválení implementace.

## Implementační kroky (až po schválení)
1. `audio.rs` — přidat `cancel()` + guard pro PTT release.
2. `hotkey.rs` — registrace Escape + handler větev, úprava `apply()`.
3. `stt.rs` — cancellation flag / ignorování výsledku po cancelu.
4. `output.rs` — guard proti zápisu po cancelu.
5. `pill.rs` + `pill.js` — event `recording-cancelled`.
6. Build + manuální test: start → Escape → ověřit žádná schránka/paste/historie/WAV.

## Verifikace
- `cargo tauri dev`, hotkey start → Escape během recording → pill zmizí, schránka nezměněná, žádný WAV, žádná historie.
- Totéž během transcribing (dle varianty A).
- PTT: podržet hotkey → Escape → pustit hotkey → nic se nestane.
- Error path: Escape při idle nedělá nic.
