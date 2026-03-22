//! Генерация и валидация JWT токенов

#![allow(dead_code)]  // extract_username может использоваться позже

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Claims внутри нашего JWT
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    pub sub: String,
    pub iat: i64,
    pub exp: i64,
}

/// Ошибки работы с JWT
#[derive(Error, Debug)]
pub enum JwtError {
    #[error("Failed to encode token: {0}")]
    EncodeError(String),
    
    #[error("Failed to decode token: {0}")]
    DecodeError(String),
    
    #[error("Token expired")]
    Expired,
    
    #[error("Invalid token")]
    Invalid,
}

// Вспомогательная функция для конвертации ошибок jsonwebtoken
impl From<jsonwebtoken::errors::Error> for JwtError {
    fn from(err: jsonwebtoken::errors::Error) -> Self {
        match err.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => JwtError::Expired,
            jsonwebtoken::errors::ErrorKind::InvalidSignature 
            | jsonwebtoken::errors::ErrorKind::InvalidToken 
            | jsonwebtoken::errors::ErrorKind::InvalidIssuer
            | jsonwebtoken::errors::ErrorKind::InvalidAudience
            | jsonwebtoken::errors::ErrorKind::InvalidSubject => JwtError::Invalid,
            _ => JwtError::DecodeError(err.to_string()),
        }
    }
}

/// Сервис для работы с JWT
#[derive(Clone)]
pub struct JwtService {
    secret: String,
    expiration_days: i64,
}

impl JwtService {
    pub fn new(secret: String, expiration_days: i64) -> Self {
        Self { secret, expiration_days }
    }
    
    /// Генерация токена для пользователя
    pub fn generate_token(&self, username: &str) -> Result<String, JwtError> {
        let now = Utc::now();
        let claims = JwtClaims {
            sub: username.to_string(),
            iat: now.timestamp(),
            exp: (now + Duration::days(self.expiration_days)).timestamp(),
        };
        
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        ).map_err(|e| JwtError::EncodeError(e.to_string()))?;
        
        Ok(token)
    }
    
    /// Валидация и декодирование токена
    pub fn verify_token(&self, token: &str) -> Result<JwtClaims, JwtError> {
        let validation = Validation::default();
        
        let token_data = decode::<JwtClaims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        ).map_err(JwtError::from)?;
        
        // Дополнительная проверка экспирации
        if token_data.claims.exp < Utc::now().timestamp() {
            return Err(JwtError::Expired);
        }
        
        Ok(token_data.claims)
    }
    
    /// Извлечение username из токена (удобный хелпер)
    pub fn extract_username(&self, token: &str) -> Result<String, JwtError> {
        let claims = self.verify_token(token)?;
        Ok(claims.sub)
    }
    
    /// Извлечение полных claims из токена (алиас для verify_token)
    pub fn extract_claims(&self, token: &str) -> Result<JwtClaims, JwtError> {
        self.verify_token(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_and_verify() {
        let service = JwtService::new("test-secret".into(), 7);
        let token = service.generate_token("testuser").unwrap();
        let claims = service.verify_token(&token).unwrap();
        assert_eq!(claims.sub, "testuser");
    }
    
    #[test]
    fn test_invalid_token() {
        let service = JwtService::new("secret1".into(), 7);
        let wrong_service = JwtService::new("secret2".into(), 7);
        let token = service.generate_token("user").unwrap();
        assert!(wrong_service.verify_token(&token).is_err());
    }
}