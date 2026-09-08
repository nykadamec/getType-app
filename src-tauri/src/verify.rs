// gettype — ověření Groq API klíče (Settings).
//
// GET na OpenAI-kompatibilní endpoint Groqu, klíč jen v Authorization
// hlavičce, nikdy do logů. Veškeré chyby jako Err(hláška pro UI).

use std::time::Duration;

const GROQ_MODELS_URL: &str = "https://api.groq.com/openai/v1/models";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Ověří API klíč proti Groq. Prázdný klíč → Err, platný → Ok("verified").
#[tauri::command]
pub async fn verify_api_key(key: String) -> Result<String, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Enter an API key first".into());
    }

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
        200 => Ok("verified".into()),
        401 => Err("Invalid API key".into()),
        429 => Err("Rate limited — try again later".into()),
        other => Err(format!("Groq error {other}")),
    }
}
