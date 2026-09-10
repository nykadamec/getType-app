// gettype — Groq Whisper STT (fáze 4).
//
// Vstupem je WAV z audio::stop. POST na OpenAI-kompatibilní endpoint Groqu,
// výsledný text předá do output::apply (clipboard/auto-paste). Veškeré chyby
// jdou jako event `transcription-error` do frontendu — žádné paniky.

use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::settings;
use crate::log;

const GROQ_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Koncový stav pilulky: CSS fade-out + zpožděné hide (idempotentní).
fn fade_out_pill(app: &AppHandle) {
    crate::pill::fade_out(app);
}

/// Chybová cesta: event do frontendu + fade-out pilulky (koncový stav).
fn emit_error(app: &AppHandle, message: impl Into<String>) {
    let _ = app.emit(
        "transcription-error",
        json!({ "message": message.into() }),
    );
    fade_out_pill(app);
}

/// Spustí transkripci v async tasku (UI thread nikdy nezastaví).
/// `generation` patří k akci z `audio::stop` — pokud mezitím přišel cancel
/// (nebo novější akce), výsledek se zahodí: žádný `history::record`, žádný
/// `output::apply`, jen návrat (pilulku už skryl cancel).
pub fn transcribe(app: AppHandle, path: PathBuf, generation: u64) {
    log::info(
        "stt",
        format!("transcribe start path=\"{}\" generation={generation}", path.to_string_lossy()),
    );
    crate::audio::set_transcribing(generation);
    tauri::async_runtime::spawn(async move {
        let result = run(&app, &path).await;
        crate::audio::clear_transcribing(generation);
        // Transkripce doběhla → Escape zpět ostatním aplikacím, jen když
        // mezitím nezačala novější akce (guard proti shazení jejího Escapu).
        crate::hotkey::release_escape_if_idle(&app);
        if crate::audio::is_stale(generation) {
            log::info("stt", format!("transcribe discarded stale=true generation={generation}"));
            return;
        }
        match result {
            Ok(text) => {
                // output::apply skryje pilulku jako svůj první krok (ať paste
                // nepřichází, dokud je okno viditelné), pak teprve event.
                // Jediný owner skrývání je output::apply — žádná duplicitní
                // pojistka tady (dvojí fade rozbíjel generační počítadlo).
                let chars = text.chars().count();
                log::info("stt", format!("transcribe done ok=true chars={chars} generation={generation}"));
                // Historie: synchronně před output::apply (rychlé, Mutex + malý JSON).
                crate::history::record(&app, &text, &crate::settings::load().model);
                crate::output::apply(&app, text.clone(), generation);
                let _ = app.emit(
                    "transcription-complete",
                    json!({ "chars": chars }),
                );
            }
            Err(message) => {
                log::error("stt", format!("transcribe failed generation={generation} err=\"{message}\""));
                emit_error(&app, message);
            }
        }
    });
}

/// Celý HTTP tok; Ok(text) nebo Err(hláška pro pilulku/stderr).
async fn run(app: &AppHandle, path: &PathBuf) -> Result<String, String> {
    // 1. API key z macOS Keychain.
    let api_key = match settings::get_api_key() {
        Ok(Some(key)) => key,
        Ok(None) => return Err("No Groq API key — add it in Settings".into()),
        Err(e) => return Err(format!("Keychain error: {e}")),
    };

    // 2. Pilulka přechází do stavu „překládám“.
    log::info("stt", "transcribing started");
    let _ = app.emit("transcribing-started", json!({}));

    // 3. request
    let config = settings::load();
    let wav = std::fs::read(path).map_err(|e| format!("Cannot read recording: {e}"))?;

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let mut form = reqwest::multipart::Form::new()
        .text("model", config.model.clone())
        .text("response_format", "json");
    if !config.language.trim().is_empty() {
        form = form.text("language", config.language.trim().to_string());
    }
    form = form.part(
        "file",
        reqwest::multipart::Part::bytes(wav)
            .file_name("rec.wav")
            .mime_str("audio/wav")
            .map_err(|e| e.to_string())?,
    );

    let response = client
        .post(GROQ_URL)
        .bearer_auth(&api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    // 4. chybové statusy s lidsky čitelnou hláškou
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = match status.as_u16() {
            401 => "Invalid API key".to_string(),
            429 => "Rate limited — try again".to_string(),
            other => format!("Groq error {other}: {}", truncate(&body, 200)),
        };
        return Err(message);
    }

    // 5. parse { "text": "..." }
    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid response from Groq: {e}"))?;
    let text = value["text"].as_str().unwrap_or_default().trim().to_string();
    if text.is_empty() {
        return Err("Empty transcription".into());
    }
    Ok(text)
}

/// Zkrátí tělo odpovědi pro hlášku (max 200 znaků, jednorázový řez na char hranici).
fn truncate(body: &str, max_chars: usize) -> String {
    if body.chars().count() <= max_chars {
        return body.to_string();
    }
    body.chars().take(max_chars).collect()
}
