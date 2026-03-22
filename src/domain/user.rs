//! Модель пользователя и репозиторий

#![allow(dead_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    
    pub email: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing)]
    pub email_verification_token: Option<String>,
    pub email_verification_token_expires_at: Option<DateTime<Utc>>,
    
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    
    pub is_active: bool,
    pub is_verified: bool,
}

impl User {
    pub fn is_email_verified(&self) -> bool {
        self.email_verified_at.is_some()
    }
    
    pub fn is_verification_token_valid(&self, token: &str) -> bool {
        self.email_verification_token.as_deref() == Some(token)
            && self.email_verification_token_expires_at.map(|exp| exp > Utc::now()).unwrap_or(false)
    }
    
    pub fn generate_verification_token() -> String {
        use rand::RngExt;
        use base64::{engine::general_purpose, Engine as _};
        let mut bytes = [0u8; 32];
        rand::rng().fill(&mut bytes);
        general_purpose::URL_SAFE_NO_PAD.encode(&bytes)
    }
}

#[derive(Debug, Clone)]
pub struct CreateUser {
    pub username: String,
    pub password_hash: String,
    pub email: String,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateUser {
    pub email: Option<String>,
    pub is_active: Option<bool>,
    pub is_verified: Option<bool>,
}

#[derive(Clone)]
pub struct UserRepository<'a> {
    db: &'a sqlx::PgPool,
}

impl<'a> UserRepository<'a> {
    pub fn new(db: &'a sqlx::PgPool) -> Self {
        Self { db }
    }
    
    pub async fn create(&self, data: CreateUser) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"INSERT INTO users (username, password_hash, email) VALUES ($1, $2, $3) RETURNING *"#
        )
        .bind(&data.username)
        .bind(&data.password_hash)
        .bind(&data.email)
        .fetch_one(self.db)
        .await
    }
    
    pub async fn find_by_username(&self, username: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"SELECT * FROM users WHERE username = $1 AND is_active = true"#
        )
        .bind(username)
        .fetch_optional(self.db)
        .await
    }
    
    pub async fn find_by_id(&self, id: i64) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"SELECT * FROM users WHERE id = $1 AND is_active = true"#
        )
        .bind(id)
        .fetch_optional(self.db)
        .await
    }
    
    pub async fn find_by_email(&self, email: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"SELECT * FROM users WHERE email = $1 AND is_active = true"#
        )
        .bind(email)
        .fetch_optional(self.db)
        .await
    }
    
    pub async fn find_by_verification_token(&self, token: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"SELECT * FROM users WHERE email_verification_token = $1 AND email_verification_token_expires_at > NOW() AND is_active = true"#
        )
        .bind(token)
        .fetch_optional(self.db)
        .await
    }
    
    pub async fn update(&self, id: i64, data: UpdateUser) -> Result<User, sqlx::Error> {
        sqlx::query_as::<_, User>(
            r#"UPDATE users SET email = COALESCE($1, email), is_active = COALESCE($2, is_active), is_verified = COALESCE($3, is_verified), updated_at = NOW() WHERE id = $4 RETURNING *"#
        )
        .bind(data.email)
        .bind(data.is_active)
        .bind(data.is_verified)
        .bind(id)
        .fetch_one(self.db)
        .await
    }
    
    pub async fn mark_email_verified(&self, user_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE users SET email_verified_at = NOW(), email_verification_token = NULL, email_verification_token_expires_at = NULL, updated_at = NOW() WHERE id = $1"#
        )
        .bind(user_id)
        .execute(self.db)
        .await?;
        Ok(())
    }
    
    pub async fn update_verification_token(&self, user_id: i64, token: &str, token_expires: DateTime<Utc>) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE users SET email_verification_token = $1, email_verification_token_expires_at = $2, updated_at = NOW() WHERE id = $3"#
        )
        .bind(token)
        .bind(token_expires)
        .bind(user_id)
        .execute(self.db)
        .await?;
        Ok(())
    }
}