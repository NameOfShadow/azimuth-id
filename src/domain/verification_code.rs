//! Сервис для генерации и проверки кодов верификации через Redis
//! Работает с: rand = "0.10", redis = "1.1"

use redis::{aio::MultiplexedConnection, AsyncCommands, Client};
use tracing::info;
use rand::{rng, RngExt};  // ← rand 0.10: RngExt для random_range
use serde::{Serialize, Deserialize};
use std::time::Duration;

/// Конфигурация сервиса кодов
#[derive(Clone)]
pub struct VerificationCodeConfig {
    pub redis_url: String,
    pub code_length: usize,
    pub ttl_minutes: u64,
    pub max_attempts: u8,
}

/// Сервис работы с кодами верификации
#[derive(Clone)]
pub struct VerificationCodeService {
    config: VerificationCodeConfig,
    client: Client,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VerificationCodeData {
    pub code: String,
    pub attempts: u8,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub email: String,
}

impl VerificationCodeService {
    pub async fn new(config: VerificationCodeConfig) -> Result<Self, String> {
        let client = Client::open(config.redis_url.clone())
            .map_err(|e| format!("Failed to create Redis client: {}", e))?;
        
        // ✅ redis 1.1: используем get_multiplexed_async_connection
        let mut conn: MultiplexedConnection = client.get_multiplexed_async_connection().await
            .map_err(|e| format!("Failed to connect to Redis: {}", e))?;
        
        // ✅ PING через AsyncCommands трейт
        let _: () = conn.ping().await
            .map_err(|e| format!("Redis ping failed: {}", e))?;
        
        info!("Connected to Redis for verification codes");
        Ok(Self { config, client })
    }

    /// Генерирует случайный код из цифр
    pub fn generate_code(&self) -> String {
        // ✅ rand 0.10: используем rand::rng() + RngExt::random_range
        let mut rng = rng();
        (0..self.config.code_length)
            .map(|_| rng.random_range(0..10).to_string())  // ✅ random_range из RngExt
            .collect()
    }

    /// Создаёт новый код для email и сохраняет в Redis
    pub async fn create_code(&self, email: &str) -> Result<String, String> {
        let code = self.generate_code();
        let expires_in = Duration::from_secs(self.config.ttl_minutes * 60);
        
        let mut conn: MultiplexedConnection = self.client.get_multiplexed_async_connection().await
            .map_err(|e| format!("Redis connection failed: {}", e))?;
        
        // ✅ ИСПРАВЛЕНО: двоеточие после code_data и правильное имя переменной
        let code_data: VerificationCodeData = VerificationCodeData {
            code: code.clone(),
            attempts: 0,
            created_at: chrono::Utc::now(),
            email: email.to_string(),
        };
        
        let json = serde_json::to_string(&code_data)
            .map_err(|e| format!("Failed to serialize code  {}", e))?;
        
        let key = format!("verify:{}", email.to_lowercase());
        
        // ✅ set_ex через AsyncCommands
        let _: () = conn.set_ex(&key, json, expires_in.as_secs()).await
            .map_err(|e| format!("Failed to save code to Redis: {}", e))?;
        
        info!(email = %email, "Verification code created (expires in {} min)", self.config.ttl_minutes);
        Ok(code)
    }

    /// Проверяет код и возвращает результат
    pub async fn verify_code(&self, email: &str, input_code: &str) -> Result<bool, String> {
        let key = format!("verify:{}", email.to_lowercase());
        let mut conn: MultiplexedConnection = self.client.get_multiplexed_async_connection().await
            .map_err(|e| format!("Redis connection failed: {}", e))?;
        
        let json: Option<String> = conn.get(&key).await
            .map_err(|e| format!("Failed to get code from Redis: {}", e))?;
        
        // ✅ ИСПРАВЛЕНО: правильное объявление переменной
        let mut code_data: VerificationCodeData = match json {
            Some(ref j) => serde_json::from_str(j)  // ← j это &String, передаём как &str
                .map_err(|e| format!("Failed to parse code  {}", e))?,
            None => return Err("Code not found or expired".to_string()),
        };
        
        // Проверка срока действия
        if code_data.created_at + chrono::Duration::minutes(self.config.ttl_minutes as i64) < chrono::Utc::now() {
            let _: () = conn.del(&key).await.unwrap_or(());
            return Err("Code has expired".to_string());
        }
        
        // Проверка попыток
        if code_data.attempts >= self.config.max_attempts {
            let _: () = conn.del(&key).await.unwrap_or(());
            return Err("Maximum attempts exceeded".to_string());
        }
        
        // Сравнение кода
        if code_data.code == input_code {
            let _: () = conn.del(&key).await.unwrap_or(());
            info!(email = %email, "Verification code verified successfully");
            Ok(true)
        } else {
            code_data.attempts += 1;
            let remaining = self.config.max_attempts - code_data.attempts;
            info!(email = %email, attempts = code_data.attempts, remaining = %remaining, "Invalid verification code attempt");
            
            let json = serde_json::to_string(&code_data)
                .map_err(|e| format!("Failed to serialize code  {}", e))?;
            let ttl = self.config.ttl_minutes * 60;
            let _: () = conn.set_ex(&key, json, ttl).await
                .map_err(|e| format!("Failed to update code attempts: {}", e))?;
            
            if remaining == 0 {
                Err("Maximum attempts exceeded".to_string())
            } else {
                Ok(false)
            }
        }
    }

    /// Удаляет код для email
    pub async fn delete_code(&self, email: &str) -> Result<(), String> {
        let mut conn: MultiplexedConnection = self.client.get_multiplexed_async_connection().await
            .map_err(|e| format!("Redis connection failed: {}", e))?;
        let key = format!("verify:{}", email.to_lowercase());
        let _: () = conn.del(&key).await
            .map_err(|e| format!("Failed to delete code: {}", e))?;
        Ok(())
    }

    /// Возвращает клиент Redis для внешних операций (rate limiting)
    pub fn client(&self) -> &Client {
        &self.client
    }
}