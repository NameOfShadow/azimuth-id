#![warn(missing_docs)]
//! Azimuth-ID Auth Client — удобный gRPC клиент для других микросервисов.
//! 
//! Этот клиент делает вызовы К сервису аутентификации, а не реализует его.

use azimuth_proto::azimuth::auth::v1::{
    auth_service_client::AuthServiceClient,
    VerifyTokenRequest, GetUserRequest, get_user_request::Identifier as UserId,
    VerifyTokenResponse, HealthRequest,
};
use tonic::transport::{Channel, Endpoint};
use tonic::Request;  // ← добавь этот импорт
#[cfg(feature = "tls")]
use tonic::transport::ClientTlsConfig;
use thiserror::Error;
use std::time::Duration;

pub use azimuth_proto::azimuth::auth::v1::GetUserResponse;

// ============================================================================
// ERRORS
// ============================================================================

/// Ошибки клиента аутентификации Azimuth-ID
#[derive(Error, Debug)]
pub enum AuthClientError {
    /// Некорректный адрес gRPC-сервера
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
    
    /// Ошибка подключения к сервису
    #[error("Failed to connect to auth service: {0}")]
    ConnectionError(#[from] tonic::transport::Error),
    
    /// Ошибка gRPC-вызова (статус от сервера)
    #[error("gRPC call failed: {0}")]
    RpcError(#[from] tonic::Status),
    
    /// Токен невалиден или истёк
    #[error("Token is invalid: {0}")]
    InvalidToken(String),
    
    /// Пользователь не найден в базе
    #[error("User not found")]
    UserNotFound,
    
    /// Внутренняя ошибка клиента
    #[error("Internal error: {0}")]
    Internal(String),
}

// ============================================================================
// TYPES
// ============================================================================

/// Результат успешной валидации токена
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VerifiedUser {
    /// Username пользователя
    pub username: String,
    /// Опциональный числовой ID пользователя
    pub user_id: Option<i64>,
    /// Timestamp истечения токена (unix seconds)
    pub expires_at: Option<i64>,
    /// Опциональная область доступа (scope)
    pub scope: Option<String>,
}

/// Клиент для взаимодействия с Azimuth-ID Auth Service
#[derive(Clone)]
pub struct AuthClient {
    client: AuthServiceClient<Channel>,
    service_name: String,  // имя сервиса для аудита на сервере
}

// ============================================================================
// IMPLEMENTATION
// ============================================================================

impl AuthClient {
    /// Подключение к auth-сервису (без TLS)
    pub async fn connect(addr: &str, service_name: &str) -> Result<Self, AuthClientError> {
        let endpoint = Endpoint::from_shared(addr.to_string())
            .map_err(|e| AuthClientError::InvalidAddress(e.to_string()))?;
        
        let channel = endpoint
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .tcp_keepalive(Some(Duration::from_secs(30)))
            .connect()
            .await?;
        
        Ok(Self {
            client: AuthServiceClient::new(channel),
            service_name: service_name.to_string(),
        })
    }

    /// Подключение с TLS (требует фичи "tls")
    #[cfg(feature = "tls")]
    pub async fn connect_tls(addr: &str, service_name: &str) -> Result<Self, AuthClientError> {
        let endpoint = Endpoint::from_shared(addr.to_string())
            .map_err(|e| AuthClientError::InvalidAddress(e.to_string()))?;
        
        let channel = endpoint
            .tls_config(ClientTlsConfig::new())?
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(10))
            .connect()
            .await?;
        
        Ok(Self {
            client: AuthServiceClient::new(channel),
            service_name: service_name.to_string(),
        })
    }

    /// Валидация JWT-токена через вызов к auth-сервису
    /// 
    /// Этот метод НЕ валидирует токен локально — он делает gRPC вызов к серверу.
    /// Сервер сам проверит подпись, blacklist, expiration и вернёт результат.
    pub async fn verify_token(&self, token: &str) -> Result<VerifiedUser, AuthClientError> {
        // Формируем запрос по протоколу
        let request = Request::new(VerifyTokenRequest {
            token: token.to_string(),
            service_name: self.service_name.clone(),  // для аудита на сервере
            required_scope: None,  // можно добавить проверку scope на сервере
        });
        
        // Делаем gRPC вызов
        let response: VerifyTokenResponse = self.client.clone()
            .verify_token(request)
            .await?  // ← здесь может быть tonic::Status ошибка
            .into_inner();
        
        // Обрабатываем ответ от сервера
        if response.valid {
            Ok(VerifiedUser {
                username: response.username.unwrap_or_default(),
                user_id: response.user_id,
                expires_at: response.expires_at,
                scope: response.scope,
            })
        } else {
            // Сервер сказал что токен невалиден
            Err(AuthClientError::InvalidToken(
                if response.error_message.is_empty() {
                    "unknown error".to_string()
                } else {
                    response.error_message
                }
            ))
        }
    }

    /// Получение пользователя по username через gRPC вызов
    pub async fn get_user_by_name(&self, username: &str) -> Result<GetUserResponse, AuthClientError> {
        let request = Request::new(GetUserRequest {
            identifier: Some(UserId::Username(username.to_string())),
        });
        
        let response = self.client.clone()
            .get_user(request)
            .await?
            .into_inner();
        
        if response.found {
            Ok(response)
        } else {
            Err(AuthClientError::UserNotFound)
        }
    }

    /// Получение пользователя по user_id через gRPC вызов
    pub async fn get_user_by_id(&self, user_id: i64) -> Result<GetUserResponse, AuthClientError> {
        let request = Request::new(GetUserRequest {
            identifier: Some(UserId::UserId(user_id)),
        });
        
        let response = self.client.clone()
            .get_user(request)
            .await?
            .into_inner();
        
        if response.found {
            Ok(response)
        } else {
            Err(AuthClientError::UserNotFound)
        }
    }

    /// Health check через gRPC вызов
    pub async fn health(&self) -> Result<azimuth_proto::azimuth::auth::v1::HealthResponse, AuthClientError> {
        let request = Request::new(HealthRequest {});
        let response = self.client.clone()
            .health(request)
            .await?
            .into_inner();
        
        Ok(response)
    }
}