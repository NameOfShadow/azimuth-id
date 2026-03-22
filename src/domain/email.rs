//! Сервис отправки писем с кодами верификации (только коды, без ссылок)

use tracing::info;
use reqwest::Client;
use serde::Serialize;

#[derive(Clone)]
pub struct EmailConfig {
    pub from_email: String,
    pub from_name: String,
    pub base_url: String,
    pub resend_api_key: Option<String>,
}

#[derive(Clone)]
pub struct EmailService {
    config: EmailConfig,
    client: Client,
}

impl EmailService {
    pub fn new(config: EmailConfig) -> Self {
        Self { config, client: Client::new() }
    }

    /// Отправляет письмо с 6-значным кодом верификации
    pub async fn send_verification_code(
        &self,
        to_email: &str,
        username: &str,
        code: &str,
    ) -> Result<(), String> {
        if let Some(api_key) = &self.config.resend_api_key {
            return self.send_code_via_resend(to_email, username, code, api_key).await;
        }

        #[cfg(debug_assertions)]
        {
            eprintln!("\n🔓 DEV MODE: Verification code for {}", to_email);
            eprintln!("   Code: {}", code);
            eprintln!("   👉 Введи этот код на странице верификации\n");
            info!(to = %to_email, code = %code, "🔓 DEV: Verification code simulated");
            return Ok(());
        }

        #[cfg(not(debug_assertions))]
        Err("No email provider configured".to_string())
    }

    async fn send_code_via_resend(
        &self,
        to_email: &str,
        username: &str,
        code: &str,
        api_key: &str,
    ) -> Result<(), String> {
        #[derive(Serialize)]
        struct ResendRequest {
            from: String,
            to: Vec<String>,
            subject: String,
            html: String,
        }

        let from = format!("{} <{}>", self.config.from_name, self.config.from_email);
        
        let request = ResendRequest {
            from,
            to: vec![to_email.to_string()],
            subject: "Код подтверждения для Azimuth".to_string(),
            html: self.verification_code_body(username, code),
        };

        info!(to = %to_email, "Sending verification code via Resend...");

        let response = self.client
            .post("https://api.resend.com/emails")  // ← БЕЗ пробелов в конце!
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Resend request failed: {}", e))?;

        let status = response.status();
        
        if status.is_success() {
            let resp: serde_json::Value = response.json().await
                .map_err(|e| format!("Failed to parse Resend response: {}", e))?;
            let email_id = resp.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
            info!(to = %to_email, resend_id = %email_id, "✅ Verification code sent via Resend");
            Ok(())
        } else {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            Err(format!("Resend API error ({}): {}", status, error_text))
        }
    }

    fn verification_code_body(&self, username: &str, code: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="utf-8"><title>Код подтверждения</title></head>
<body style="font-family:-apple-system,BlinkMacSystemFont,sans-serif;max-width:600px;margin:0 auto;padding:20px;background:#f5f5f7;color:#1d1d1f;text-align:center;">
<div style="background:white;border-radius:12px;padding:32px;box-shadow:0 2px 12px rgba(0,0,0,0.1);">
<h2 style="margin:0 0 16px 0;color:#007bff;">🧭 Azimuth</h2>
<p style="font-size:16px;line-height:1.5;margin-bottom:24px;">👋 Привет, <strong>{username}</strong>!<br>Используй этот код для подтверждения аккаунта:</p>
<div style="margin:32px 0;"><div style="background:linear-gradient(135deg,#667eea 0%,#764ba2 100%);padding:20px 32px;border-radius:12px;font-size:32px;font-weight:700;letter-spacing:12px;color:white;display:inline-block;font-family:'Courier New',monospace;box-shadow:0 4px 20px rgba(102,126,234,0.4);">{code}</div></div>
<p style="color:#666;font-size:14px;">🔐 Код действителен <strong>15 минут</strong><br>Не сообщай этот код никому</p>
<hr style="border:none;border-top:1px solid #eee;margin:32px 0;">
<p style="color:#86868b;font-size:13px;">Если вы не регистрировались в Azimuth — просто проигнорируйте это письмо.</p>
<p style="color:#86868b;font-size:12px;margin-top:24px;">© 2026 Azimuth. All rights reserved.</p>
</div>
</body></html>"#,
            username = username, code = code
        )
    }
}