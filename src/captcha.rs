//! Верификация CAPTCHA (Cloudflare Turnstile)

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::warn;

/// Ответ от Cloudflare Turnstile API
#[derive(Debug, Deserialize)]
struct TurnstileResponse {
    success: bool,
    #[serde(default)]
    error_codes: Vec<String>,
}

/// Запрос к Turnstile API
#[derive(Serialize)]
struct TurnstileVerifyRequest {
    secret: String,
    response: String,
    remoteip: Option<String>,
}

/// Сервис для работы с CAPTCHA
#[derive(Clone)]
pub struct CaptchaService {
    client: Client,
    secret_key: String,
    verify_url: String,
}

impl CaptchaService {
    pub fn new(secret_key: String) -> Self {
        Self {
            client: Client::new(),
            secret_key,
            verify_url: "https://challenges.cloudflare.com/turnstile/v0/siteverify".to_string(),
        }
    }

    /// Верификация токена от виджета
    pub async fn verify(&self, token: &str, remote_ip: Option<&str>) -> Result<bool, String> {
        let request = TurnstileVerifyRequest {
            secret: self.secret_key.clone(),
            response: token.to_string(),
            remoteip: remote_ip.map(|s| s.to_string()),
        };

        let response = self.client
            .post(&self.verify_url)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Turnstile request failed: {}", e))?;

        let result: TurnstileResponse = response
            .json()
            .await
            .map_err(|e| format!("Turnstile parse failed: {}", e))?;

        if !result.success {
            warn!("Turnstile verification failed: {:?}", result.error_codes);
            return Ok(false);
        }

        Ok(true)
    }
}