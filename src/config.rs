//! Конфигурация приложения (загружается из переменных окружения)

use serde::Deserialize;

#[derive(Clone, Deserialize)]
pub struct Config {
    // ===== Обязательные поля =====
    pub database_url: String,
    pub jwt_secret: String,
    
    // ===== Адреса серверов =====
    pub grpc_addr: String,
    pub http_addr: String,
    
    // ===== JWT настройки =====
    pub jwt_expiration_days: i64,
    
    // ===== Cloudflare Turnstile (опционально) =====
    pub turnstile_secret: Option<String>,
    
    // ===== Email config (Resend) =====
    pub resend_api_key: Option<String>,
    pub from_email: String,
    pub from_name: String,
    pub base_url: String,
    
    // ===== Redis config для кодов верификации =====
    pub redis_url: Option<String>,  // ← ДОБАВЬ ЭТО ПОЛЕ
}

impl Config {
    pub fn load() -> Result<Self, String> {
        Ok(Self {
            // ===== Обязательные =====
            database_url: std::env::var("DATABASE_URL")
                .map_err(|e| format!("DATABASE_URL not set: {}", e))?,
            
            jwt_secret: std::env::var("JWT_SECRET")
                .map_err(|e| format!("JWT_SECRET not set: {}", e))?,
            
            // ===== Адреса (с дефолтами) =====
            grpc_addr: std::env::var("GRPC_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:50051".to_string()),
            
            http_addr: std::env::var("HTTP_ADDR")
                .unwrap_or_else(|_| "0.0.0.0:3000".to_string()),
            
            // ===== JWT (с дефолтом 7 дней) =====
            jwt_expiration_days: std::env::var("JWT_EXPIRATION_DAYS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(7),
            
            // ===== Опциональные поля =====
            turnstile_secret: std::env::var("TURNSTILE_SECRET_KEY").ok(),
            resend_api_key: std::env::var("RESEND_API_KEY").ok(),
            
            // ===== Отправитель (с дефолтами) =====
            from_email: std::env::var("FROM_EMAIL")
                .unwrap_or_else(|_| "noreply@azimuth.local".to_string()),
            
            from_name: std::env::var("FROM_NAME")
                .unwrap_or_else(|_| "Azimuth".to_string()),
            
            base_url: std::env::var("BASE_URL")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
            
            // ===== Redis (опционально) =====
            redis_url: std::env::var("REDIS_URL").ok(),  // ← ДОБАВЬ ЭТО
        })
    }
    
    pub fn email_enabled(&self) -> bool {
        self.resend_api_key.is_some()
    }

    // ===== НОВЫЙ ХЕЛПЕР ДЛЯ ПРОВЕРКИ REDIS =====
    pub fn verification_code_enabled(&self) -> bool {
        self.redis_url.is_some()
    }
}