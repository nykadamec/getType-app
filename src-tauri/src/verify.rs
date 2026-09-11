// gettype — ověření Groq API klíče (Settings).
//
// GET na OpenAI-kompatibilní endpoint Groqu, klíč jen v Authorization
// hlavičce, nikdy do logů. Veškeré chyby jako Err(hláška pro UI).

use std::time::Duration;

const GROQ_MODELS_URL: &str = "https://api.groq.com/openai/v1/models";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Výsledek testu uloženého klíče — vždy Ok, stav nese `ok`.
#[derive(serde::Serialize)]
pub struct LoadedResult {
    pub ok: bool,
    pub message: String,
}

/// Společné ověření klíče proti Groq. Ok(()) = platný, Err(hláška pro UI) jinak.
async fn check_key(key: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| format!("Network error: {e}"))?;

    let response = client
        .get(GROQ_MODELS_URL)
        .bearer_auth(key)
        .send()
        .await
        .map_err(|e| format!("Network error: {e}"))?;

    match response.status().as_u16() {
        200 => Ok(()),
        401 => Err("Invalid API key".into()),
        429 => Err("Rate limited — try again later".into()),
        other => Err(format!("Groq error {other}")),
    }
}

/// Ověří API klíč proti Groq. Prázdný klíč → Err, platný → Ok("verified").
#[tauri::command]
pub async fn verify_api_key(key: String) -> Result<String, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Enter an API key first".into());
    }

    check_key(key).await?;
    Ok("verified".into())
}

/// Otestuje klíč uložený v Keychainu. Žádný klíč → Ok(ok=false),
/// neplatný klíč → Ok(ok=false, důvod), platný → Ok(ok=true).
/// Err jen při selhání samotného Keychainu / sítě mimo HTTP odpověď
/// (síťové chyby z check_key se vrací jako ok=false s hláškou).
#[tauri::command]
pub async fn test_saved_key() -> Result<LoadedResult, String> {
    let saved = crate::settings::get_api_key()?;
    let Some(key) = saved else {
        return Ok(LoadedResult {
            ok: false,
            message: "No API key saved".into(),
        });
    };
    let key = key.trim();
    if key.is_empty() {
        return Ok(LoadedResult {
            ok: false,
            message: "No API key saved".into(),
        });
    }

    match check_key(key).await {
        Ok(()) => Ok(LoadedResult {
            ok: true,
            message: "verified".into(),
        }),
        Err(message) => Ok(LoadedResult { ok: false, message }),
    }
}
