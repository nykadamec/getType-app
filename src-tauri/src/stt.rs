// gettype — Groq Whisper STT (fáze 4).
//
// Vstupem je WAV z audio::stop. POST na OpenAI-kompatibilní endpoint Groqu,
// výsledný text předá do output::apply (clipboard/auto-paste). Veškeré chyby
// jdou jako event `transcription-error` do frontendu — žádné paniky.

use serde_json::json;
use std::sync::OnceLock;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::settings;
use crate::log;

const GROQ_URL: &str = "https://api.groq.com/openai/v1/audio/transcriptions";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Sdílený HTTP klient pro všechny transkripce (Fáze A1).
/// `Client::builder().build()` je drahé (TLS setup) a bez reuse přichází
/// každá dikce o keep-alive spojení. Cachuje se i chyba buildu — proces by
/// stejně bez TLS nic neposlal, ale nepanikujeme (styl celého backendu).
fn http_client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    match CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .pool_idle_timeout(Duration::from_secs(90))
            .pool_max_idle_per_host(4)
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))
    }) {
        Ok(client) => Ok(client),
        Err(e) => Err(e.clone()),
    }
}

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
/// `wav` jsou hotové WAV byty z `audio::stop` — žádný disk-roundtrip (A2).
pub fn transcribe(app: AppHandle, wav: Vec<u8>, generation: u64) {
    log::info(
        "stt",
        format!("transcribe start bytes={} generation={generation}", wav.len()),
    );
    crate::audio::set_transcribing(generation);
    tauri::async_runtime::spawn(async move {
        let result = run(&app, wav).await;
        crate::audio::clear_transcribing(generation);
        // Transkripce doběhla → Escape zpět ostatním aplikacím, jen když
        // mezitím nezačala novější akce (guard proti shazení jejího Escapu).
        crate::hotkey::release_escape_if_idle(&app);
        if crate::audio::is_stale(generation) {
            log::info("stt", format!("transcribe discarded stale=true generation={generation}"));
            return;
        }
        match result {
            Ok((text, api_key)) => {
                // output::apply skryje pilulku jako svůj první krok (ať paste
                // nepřichází, dokud je okno viditelné), pak teprve event.
                // Jediný owner skrývání je output::apply — žádná duplicitní
                // pojistka tady (dvojí fade rozbíjel generační počítadlo).
                // AI post-process (Task 5): když zapnuto a akce není passthrough,
                // přepis jde přes llm::post_process se stejným klíčem jako STT
                // (žádný druhý Keychain read). Chyba → warn + ai-failed + raw fallback.
                let cfg = crate::settings::load();
                let (final_text, ai_action_taken): (String, Option<String>) =
                    if cfg.ai_enabled
                        && cfg.ai_default_action != "passthrough"
                        && !text.is_empty()
                    {
                        let _ = app.emit("ai-processing", json!({}));
                        match crate::llm::post_process(
                            &text,
                            &cfg.ai_default_action,
                            &cfg.ai_model,
                            &api_key,
                        )
                        .await
                        {
                            Ok(out) => (out, Some(cfg.ai_default_action.clone())),
                            Err(e) => {
                                log::warn("ai", format!("fallback raw err=\"{e}\""));
                                let _ = app.emit("ai-failed", json!({"message": e}));
                                (text.clone(), None)
                            }
                        }
                    } else {
                        (text.clone(), None)
                    };
                let chars = final_text.chars().count();
                log::info("stt", format!("transcribe done ok=true chars={chars} generation={generation}"));
                // Historie: synchronně před output::apply (rychlé, Mutex + malý JSON).
                // raw_text jen když AI proběhlo a výsledek se liší od raw —
                // jinak None (passthrough/fallback/shodný text → bez badge).
                let raw_opt: Option<&str> = match &ai_action_taken {
                    Some(_) if text != final_text => Some(text.as_str()),
                    _ => None,
                };
                crate::history::record(
                    &app,
                    &final_text,
                    raw_opt,
                    &cfg.model,
                    ai_action_taken.as_deref(),
                );
                crate::output::apply(&app, final_text.clone(), generation);
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

/// Celý HTTP tok; Ok((text, api_key)) nebo Err(hláška pro pilulku/stderr).
/// `api_key` se vrací volajícímu, aby ho AI krok (llm::post_process) reuse-nul
/// bez druhého Keychain readu (Task 5).
async fn run(app: &AppHandle, wav: Vec<u8>) -> Result<(String, String), String> {
    // 1. API key z macOS Keychain.
    let api_key = match settings::get_api_key() {
        Ok(Some(key)) => key,
        Ok(None) => return Err("No Groq API key — add it in Settings".into()),
        Err(e) => return Err(format!("Keychain error: {e}")),
    };

    // 2. Pilulka přechází do stavu „překládám“.
    log::info("stt", "transcribing started");
    let _ = app.emit("transcribing-started", json!({}));

    // 3. request (sdílený klient s keep-alive — A1, WAV byty z paměti — A2)
    let config = settings::load();
    let client = http_client()?;

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
    Ok((text, api_key))
}

/// Zkrátí tělo odpovědi pro hlášku (max 200 znaků, jednorázový řez na char hranici).
fn truncate(body: &str, max_chars: usize) -> String {
    if body.chars().count() <= max_chars {
        return body.to_string();
    }
    body.chars().take(max_chars).collect()
}
