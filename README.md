# getType ⌨️🎙️

Press a hotkey, speak, done — **getType** transcribes your voice and pastes the text where your cursor is. A lightweight dictation app for macOS, living quietly in your menu bar.

## How it works

1. **Hold** (or tap) the global hotkey → a small pill appears and records your mic.
2. **Release** → the recording goes to Groq speech-to-text (`whisper-large-v3-turbo`).
3. The text is **auto-pasted** at your cursor (⌘V simulation) and copied to clipboard. Your previous clipboard content is restored afterwards.

No Dock icon. No windows in your way. Just talk.

## Features

| Area | What you get |
|---|---|
| 🎙️ Push-to-talk & toggle | Hold-to-speak or tap-to-start / tap-to-stop hotkey modes |
| 💊 Mini pill | Idle → recording → transcribing states, always on top, never steals focus |
| ⎋ ESC to cancel | Cancels recording or transcription — nothing saved, nothing pasted |
| 📋 Auto-paste + clipboard | Pastes at cursor, copies to clipboard, restores previous content |
| 🤖 AI post-processing | Optional **cleanup** (punctuation, filler words), **summary** (2–3 sentences), or **translation to English** via Groq LLM (`llama-3.1-8b-instant`, same API key). Any failure falls back to raw transcript — paste is never blocked |
| 🕘 History | Past transcriptions in Settings with click-to-copy, delete, clear-all |
| 🧭 Onboarding | First-run window: API key, permissions, and hotkey test in one place |
| ⚙️ Settings | Groq API key (Keychain, never in files), hotkey, mode, STT + AI model, language, auto-paste/copy toggles, launch at login |
| 🔐 Permissions | Microphone + Accessibility status in one place (Accessibility is required for ⌘V paste) |

## Screenshots

![Settings](assets/screenshots/settings.png)

## Requirements

- **macOS 26+** (Tahoe and newer)
- **Free Groq API key** — get one at [console.groq.com](https://console.groq.com), paste it in Settings (one-click verification included)
- Permissions: **Microphone** + **Accessibility** (System Settings → Privacy & Security)

## Development

```bash
npm install -g @tauri-apps/cli   # or: cargo install tauri-cli
cargo tauri dev                   # develop
cargo tauri build                 # build (.app / .dmg)
```

Quick smoke test: hotkey → record → release → text appears in TextEdit or Safari.

## Project structure

- `src/` — frontend (vanilla HTML/CSS/JS): `pill.*`, `popover.*`, `settings.*`, `onboarding.*`
- `src-tauri/src/` — Rust backend: `audio.rs` (capture), `hotkey.rs`, `stt.rs` (Groq Whisper), `llm.rs` (AI actions), `output.rs` (clipboard + paste), `history.rs`, `permissions.rs`, `pill.rs`, `settings.rs`, `verify.rs`, `sound.rs`
- `plan-*.md` — feature plans (in Czech)
- `designs/` — UI explorations

## Status

**v0.1.1** — MVP done: record → transcribe → paste, history, ESC cancel, AI post-processing, onboarding, Settings with Dock icon while open.
