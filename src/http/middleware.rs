//! Middleware для проверки JWT в заголовке Authorization

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{Response, Json},
};
use tracing::{info, warn};

use crate::domain::{AppState, JwtError};

/// Middleware для проверки JWT
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let auth_header = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    
    let bearer = auth_header
        .ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "error": "missing_token",
            "message": "Authorization header required"
        }))))?;
    
    if !bearer.starts_with("Bearer ") {
        return Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({
            "error": "invalid_format",
            "message": "Expected 'Bearer <token>'"
        }))));
    }
    
    let token = &bearer[7..];
    
    match state.jwt.verify_token(token) {
        Ok(claims) => {
            // Клонируем username перед вставкой в extensions
            let username = claims.sub.clone();
            request.extensions_mut().insert(username);
            info!(username = %claims.sub, "Authenticated request");
            Ok(next.run(request).await)
        }
        Err(JwtError::Expired) => {
            Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "error": "token_expired",
                "message": "Token has expired"
            }))))
        }
        Err(JwtError::Invalid) => {
            Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "error": "invalid_token",
                "message": "Token signature is invalid"
            }))))
        }
        Err(JwtError::DecodeError(msg)) | Err(JwtError::EncodeError(msg)) => {
            warn!("Token error: {}", msg);
            Err((StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "error": "token_error",
                "message": msg
            }))))
        }
    }
}

/// Экстрактор для получения текущего пользователя
pub struct VerifiedUser(pub String);

impl<S> axum::extract::FromRequestParts<S> for VerifiedUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<serde_json::Value>);
    
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<String>()
            .cloned()
            .map(VerifiedUser)
            .ok_or((StatusCode::UNAUTHORIZED, Json(serde_json::json!({
                "error": "not_authenticated",
                "message": "User not found in request"
            }))))
    }
}