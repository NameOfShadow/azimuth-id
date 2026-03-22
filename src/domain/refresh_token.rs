//! Управление refresh токенами

#![allow(dead_code)]  // token_hash используется через FromRow, не через прямой доступ

use chrono::{Duration, Utc};
use rand::RngExt;
use sha2::{Sha256, Digest};
use sqlx::FromRow;
use serde::{Deserialize, Serialize};
use base64::{engine::general_purpose, Engine as _};

/// Refresh токен в системе
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct RefreshToken {
    pub id: i64,
    pub user_id: i64,
    #[serde(skip_serializing)]
    pub token_hash: String,
    
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    
    pub expires_at: chrono::DateTime<Utc>,
    pub created_at: chrono::DateTime<Utc>,
    pub last_used_at: Option<chrono::DateTime<Utc>>,
    
    pub is_revoked: bool,
}

/// Данные для создания нового refresh токена
pub struct CreateRefreshToken {
    pub user_id: i64,
    pub token: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub expires_in_days: i64,
}

impl RefreshToken {
    /// Генерация безопасного токена (32 байта = ~43 символа base64url)
    pub fn generate() -> String {
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        general_purpose::URL_SAFE_NO_PAD.encode(&bytes)
    }
    
    /// Хеш токена для хранения в БД
    pub fn hash_token(token: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(token.as_bytes());
        hex::encode(hasher.finalize())
    }
    
    /// Проверка: валиден ли токен
    pub fn is_valid(&self) -> bool {
        !self.is_revoked && self.expires_at > Utc::now()
    }
}

/// Репозиторий для refresh токенов
pub struct RefreshTokenRepository<'a> {
    db: &'a sqlx::PgPool,
}

impl<'a> RefreshTokenRepository<'a> {
    pub fn new(db: &'a sqlx::PgPool) -> Self {
        Self { db }
    }
    
    pub async fn create(&self, data: CreateRefreshToken) -> Result<RefreshToken, sqlx::Error> {
        let token_hash = RefreshToken::hash_token(&data.token);
        let expires_at = Utc::now() + Duration::days(data.expires_in_days);
        
        sqlx::query_as::<_, RefreshToken>(
            r#"
            INSERT INTO refresh_tokens 
                (user_id, token_hash, user_agent, ip_address, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#
        )
        .bind(data.user_id)
        .bind(token_hash)
        .bind(data.user_agent)
        .bind(data.ip_address)
        .bind(expires_at)
        .fetch_one(self.db)
        .await
    }
    
    pub async fn find_by_hash(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error> {
        sqlx::query_as::<_, RefreshToken>(
            r#"SELECT * FROM refresh_tokens WHERE token_hash = $1 AND NOT is_revoked"#
        )
        .bind(token_hash)
        .fetch_optional(self.db)
        .await
    }
    
    pub async fn mark_used(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE refresh_tokens SET last_used_at = NOW() WHERE id = $1"#
        )
        .bind(id)
        .execute(self.db)
        .await?;
        Ok(())
    }
    
    pub async fn revoke(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE refresh_tokens SET is_revoked = TRUE WHERE id = $1"#
        )
        .bind(id)
        .execute(self.db)
        .await?;
        Ok(())
    }
    
    pub async fn revoke_all_for_user(&self, user_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE refresh_tokens SET is_revoked = TRUE WHERE user_id = $1"#
        )
        .bind(user_id)
        .execute(self.db)
        .await?;
        Ok(())
    }
}