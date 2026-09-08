# gettype — MVP plán

macOS menu-bar aplikace pro diktaci: globální hotkey → nahrání mikrofonu → Groq speech-to-text → auto-paste do aktivní aplikace + schránka.

## Schválený rozsah (MVP)

- **Ovládání:** push-to-talk (podrž → mluv → pusť) i toggle (tap → tap) přes globální hotkey
- **Výstup:** simulace ⌘V do aktivní aplikace + kopie do schránky
- **Nastavení:** Groq API key, hotkey, režim (PTT/toggle)
- **Bez:** historie, AI post-processing, modes, překlad

## Stack

- **Tauri 2** (Rust backend + web frontend), macOS 13+
- Appka bez Dock ikony — LSUIElement / NSApplication.accessory, jen menu bar (Tauri tray)

## Architektura

### Frontend (web UI)
- Popover/tray okno: stavy idle → recording → transcribing (viz UI návrh v gettype.pen)
- Settings okno: API key, hotkey recorder, režim, auto-paste/copy toggly, model
- Komunikace s Rust jádrem přes Tauri commands + eventy

### Rust backend
- **Audio capture:** `cpal` — input stream → 16 kHz mono WAV do dočasného souboru/buffru
  - TODO detail: převzorkování (rubato/simple), zvolit wav pro nejnižší latenci uploadu
- **Globální hotkey:** `tauri-plugin-global-shortcut` (keydown/keyup eventy → PTT i toggle)
- **Stroj stavů:** `idle → recording → transcribing → paste/error`
- **Groq klient:** `POST https://api.groq.com/openai/v1/audio/transcriptions`
  - multipart: `file=@audio.wav`, `model=whisper-large-v3-turbo`, `language=cs`, `response_format=json`
  - odpověď `{ "text": "..." }`; limit 25 MB free tier, min billed 10 s
  - API key v macOS Keychain (ne v plain configu)
- **Paste:** klávesová simulace ⌘V přes CGEvent (crate `core-graphics`) + schránka přes `arboard`/NSPasteboard
  - vyžaduje Accessibility oprávnění (AXIsProcessTrusted)
  - před paste skrýt vlastní okno, krátký delay pro focus; obnovit původní schránku po vložení (changeCount)
- **Config:** JSON v `~/Library/Application Support/gettype/` (hotkey, režim, toggly); API key → Keychain

### Oprávnění (Info.plist / TCC)
- `NSMicrophoneUsageDescription`
- Accessibility (System Settings → Privacy → Accessibility) — kvůli simulaci ⌘V
- Netřeba App Sandbox (CGEvent paste v sandboxu nefunguje); distribuce Developer ID + notarizace (později)

## Reference
- superwhisper.com/docs — UX vzory (docs/get-started)
- console.groq.com/docs/speech-to-text — API
- github.com/Rkaede/echo, github.com/richardwu/openwhisper — stejná architektura ve Swift (inspirace pro tok paste/clipboard)

## Vizual
- UI návrh: `~/Documents/gettype.pen` (light minimal, monochrome, Inter)
- Obrazovky: 01 Popover Idle, 02 Popover Recording, 03 Popover Transcribing, 04 Settings

## Stav
- [x] UI návrh schválen uživatelem (gettype.pen, 4 obrazovky, @designer)
- [ ] Jazyk: pevně `cs` vs. auto-detect (Groq umí vynechat `language` → auto)
- [ ] První spuštění onboarding (požádat o mic + accessibility oprávnění)

## Implementační fáze
1. **Scaffold** — Tauri 2 projekt (`gettype`), tray/menu bar, žádná Dock ikona, nové git repo
2. **Settings MVP** — API key (Keychain), hotkey recorder, režim PTT/toggle, config JSON
3. **Audio capture** — cpal input → 16 kHz mono WAV, start/stop přes hotkey
4. **Groq STT** — multipart upload, whisper-large-v3-turbo, `{text}` parsování, error stavy
5. **Output** — schránka (arboard) + simulace ⌘V (CGEvent), obnova schránky, Accessibility oprávnění flow
6. **UI wiring** — popover stavy (idle/recording/transcribing), mini pilulky dle návrhu
7. **Polish** — ikona, audio cues, error toasty, build + ad-hoc signing

## Verifikace
- Build: `cargo tauri dev` / `cargo tauri build`
- Manuální smoke test: hotkey → nahrání → transkripce → paste do TextEdit/Safari
- Error path: neplatný API key, offline, prázdná nahrávka (<1 s)
