//! HTTP хендлеры для клиентского API (верификация через 6-значный код)

#![allow(dead_code)]

use crate::http::middleware::VerifiedUser;
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use tracing::{info, warn};
use redis::aio::MultiplexedConnection;

use crate::domain::{AppState, CreateUser, User};

// ============================================================================
// REQUEST/RESPONSE SCHEMAS
// ============================================================================

#[derive(Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: String,
    pub cf_turnstile_response: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub cf_turnstile_response: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Serialize, ToSchema)]
pub struct RegisterResponse {
    pub message: String,
    pub email_verification_required: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct VerifyCodeRequest {
    pub email: String,
    pub code: String,  // 6-значный код
}

#[derive(Serialize, ToSchema)]
pub struct VerifyCodeResponse {
    pub message: String,
    pub username: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct ResendVerificationRequest {
    pub email: String,
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateProfileRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_verified: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct UpdateProfileResponse {
    pub username: String,
    pub is_verified: bool,
    #[schema(value_type = String, format = DateTime)]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize, ToSchema)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
}

// ============================================================================
// HANDLERS
// ============================================================================

#[utoipa::path(get, path = "/health", tag = "health", responses((status = 200, description = "Service is healthy", body = HealthResponse)))]
pub async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, Json(HealthResponse { 
        status: "ok".into(), 
        version: env!("CARGO_PKG_VERSION").into() 
    })).into_response()
}

// ===== ВАЖНО: закрывающая скобка функции =====
fn is_valid_email(email: &str) -> bool {
    email.contains('@') && email.split('@').count() == 2 && 
    email.len() <= 255 && !email.starts_with('@') && !email.ends_with('@')
}  // ← ЭТА СКОБКА БЫЛА ПРОПУЩЕНА!

// ===== RATE LIMITING ДЛЯ RESEND (60 секунд между запросами) =====

async fn check_resend_cooldown(state: &AppState, email: &str) -> Result<(), StatusCode> {
    let Some(ref code_service) = state.verification_code else { 
        return Ok(()); // если сервис не настроен — пропускаем проверку
    };
    
    let key = format!("resend_cooldown:{}", email.to_lowercase());
    let mut conn: MultiplexedConnection = match code_service.client().get_multiplexed_async_connection().await {
        Ok(c) => c, 
        Err(_) => return Ok(()), // если не можем подключиться — пропускаем
    };
    
    let exists: bool = match redis::cmd("EXISTS").arg(&key).query_async(&mut conn).await {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    
    if exists {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(())
}

async fn set_resend_cooldown(state: &AppState, email: &str) {
    let Some(ref code_service) = state.verification_code else { return; };
    let key = format!("resend_cooldown:{}", email.to_lowercase());
    let mut conn: MultiplexedConnection = match code_service.client().get_multiplexed_async_connection().await {
        Ok(c) => c, 
        Err(_) => return,
    };
    // Устанавливаем ключ с TTL 60 секунд
    let _: () = redis::cmd("SET").arg(&key).arg("1").arg("EX").arg(60)
        .query_async(&mut conn)
        .await
        .unwrap_or(());
}

// ===== REGISTER HANDLER — ОТПРАВЛЯЕТ КОД, НЕ ССЫЛКУ =====

#[utoipa::path(post, path = "/auth/register", tag = "auth", request_body = RegisterRequest, responses((status = 201), (status = 400), (status = 409), (status = 500)))]
pub async fn register_handler(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> impl IntoResponse {
    if let Err(status) = check_captcha(&state, payload.cf_turnstile_response.as_deref()).await {
        return (status, Json("CAPTCHA verification failed")).into_response();
    }

    if !is_valid_email(&payload.email) {
        return (StatusCode::BAD_REQUEST, Json("Invalid email format")).into_response();
    }
    if payload.username.is_empty() || payload.username.len() > 255 {
        return (StatusCode::BAD_REQUEST, Json("Invalid username")).into_response();
    }
    if payload.password.len() < 8 {
        return (StatusCode::BAD_REQUEST, Json("Password too short")).into_response();
    }

    let password_hash = match tokio::task::spawn_blocking(move || bcrypt::hash(payload.password, bcrypt::DEFAULT_COST)).await {
        Ok(Ok(hash)) => hash,
        _ => return (StatusCode::INTERNAL_SERVER_ERROR, Json("Failed to hash password")).into_response(),
    };

    let repo = state.user_repo();
    match repo.create(CreateUser {
        username: payload.username.clone(),
        password_hash,
        email: payload.email.clone(),
    }).await {
        Ok(user) => {
            // ===== ОТПРАВЛЯЕМ КОД ЧЕРЕЗ REDIS + EMAIL =====
            if let Some(ref code_service) = state.verification_code {
                match code_service.create_code(&payload.email).await {
                    Ok(code) => {
                        if let Some(ref email_service) = state.email {
                            let _ = email_service
                                .send_verification_code(&payload.email, &payload.username, &code)
                                .await;
                        }
                    }
                    Err(e) => tracing::warn!("Failed to create verification code: {}", e),
                }
            }
            
            info!(username = %user.username, email = %payload.email, "User registered");
            (StatusCode::CREATED, Json(RegisterResponse {
                message: "Registration successful. Please check your email for a verification code.".into(),
                email_verification_required: true,
            })).into_response()
        }
        Err(sqlx::Error::Database(db_err)) if db_err.is_unique_violation() => {
            warn!(username = %payload.username, "Registration failed: username or email exists");
            (StatusCode::CONFLICT, Json("Username or email already exists")).into_response()
        }
        Err(e) => {
            warn!("Registration failed: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json("Internal error")).into_response()
        }
    }
}

// ===== LOGIN HANDLER =====

#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = LoginRequest, responses((status = 200, body = LoginResponse), (status = 401), (status = 403), (status = 500)))]
pub async fn login_handler(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    if let Err(status) = check_captcha(&state, payload.cf_turnstile_response.as_deref()).await {
        return (status, Json("CAPTCHA verification failed")).into_response();
    }

    let repo = state.user_repo();
    let user = match repo.find_by_username(&payload.username).await {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::UNAUTHORIZED, Json("Invalid credentials")).into_response(),
        Err(e) => {
            warn!("Login failed: database error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json("Database error")).into_response();
        }
    };

    if !user.is_email_verified() {
        return (StatusCode::FORBIDDEN, Json(serde_json::json!({
            "error": "email_not_verified",
            "message": "Please verify your email before logging in",
            "resend_endpoint": "/auth/resend-verification"
        }))).into_response();
    }

    let password_valid = match tokio::task::spawn_blocking(move || bcrypt::verify(payload.password, &user.password_hash)).await {
        Ok(Ok(valid)) => valid,
        _ => return (StatusCode::INTERNAL_SERVER_ERROR, Json("Internal error")).into_response(),
    };

    if !password_valid {
        return (StatusCode::UNAUTHORIZED, Json("Invalid credentials")).into_response();
    }

    match state.jwt.generate_token(&user.username) {
        Ok(token) => {
            info!(username = %user.username, "Login successful");
            (StatusCode::OK, Json(LoginResponse { token })).into_response()
        }
        Err(e) => {
            warn!("Failed to generate token: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json("Token generation failed")).into_response()
        }
    }
}

// ===== НОВЫЙ: VERIFY CODE HANDLER (верификация через 6-значный код) =====

#[utoipa::path(
    post, 
    path = "/auth/verify-code", 
    tag = "auth", 
    request_body = VerifyCodeRequest, 
    responses(
        (status = 200, body = VerifyCodeResponse), 
        (status = 400), 
        (status = 404),
        (status = 410),
        (status = 429)
    )
)]
pub async fn verify_code_handler(
    State(state): State<AppState>,
    Json(payload): Json<VerifyCodeRequest>,
) -> impl IntoResponse {
    // Валидация: код должен быть ровно 6 цифр
    if payload.code.len() != 6 || !payload.code.chars().all(|c| c.is_ascii_digit()) {
        return (StatusCode::BAD_REQUEST, Json("Code must be 6 digits")).into_response();
    }
    
    let code_service = match &state.verification_code {
        Some(s) => s,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, Json("Verification service not configured")).into_response(),
    };
    
    match code_service.verify_code(&payload.email, &payload.code).await {
        Ok(true) => {
            // ✅ Код верный — активируем пользователя
            let repo = state.user_repo();
            let user = match repo.find_by_email(&payload.email).await {
                Ok(Some(u)) => u,
                _ => return (StatusCode::NOT_FOUND, Json("User not found")).into_response(),
            };
            
            match repo.mark_email_verified(user.id).await {
                Ok(_) => {
                    info!(username = %user.username, email = %payload.email, "Email verified via code");
                    let _ = code_service.delete_code(&payload.email).await;
                    (StatusCode::OK, Json(VerifyCodeResponse {
                        message: "Email verified successfully. You can now login.".into(),
                        username: Some(user.username),
                    })).into_response()
                }
                Err(e) => {
                    tracing::error!("Failed to verify email: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, Json("Failed to verify email")).into_response()
                }
            }
        }
        Ok(false) => {
            // ❌ Неверный код, но есть ещё попытки
            (StatusCode::BAD_REQUEST, Json(serde_json::json!({
                "error": "invalid_code",
                "message": "Invalid code. Please try again.",
            }))).into_response()
        }
        Err(e) => {
            // ❌ Код истёк или превышены попытки
            let status = if e.contains("expired") { 
                StatusCode::GONE
            } else if e.contains("Maximum attempts") { 
                StatusCode::TOO_MANY_REQUESTS
            } else { 
                StatusCode::BAD_REQUEST 
            };
            (status, Json(serde_json::json!({ 
                "error": "verification_failed", 
                "message": e 
            }))).into_response()
        }
    }
}

// ===== RESEND VERIFICATION — С RATE LIMITING (60 сек кулдаун) =====

#[utoipa::path(
    post, 
    path = "/auth/resend-verification", 
    tag = "auth", 
    request_body = ResendVerificationRequest, 
    responses(
        (status = 200), 
        (status = 400), 
        (status = 404),
        (status = 429)
    )
)]
pub async fn resend_verification_handler(
    State(state): State<AppState>,
    Json(payload): Json<ResendVerificationRequest>,
) -> impl IntoResponse {
    if !is_valid_email(&payload.email) {
        return (StatusCode::BAD_REQUEST, Json("Invalid email format")).into_response();
    }
    
    // 🔐 RATE LIMITING: проверяем кулдаун (60 секунд)
    if let Err(status) = check_resend_cooldown(&state, &payload.email).await {
        return (status, Json(serde_json::json!({
            "error": "too_many_requests",
            "message": "Please wait before requesting another code",
            "retry_after_seconds": 60
        }))).into_response();
    }
    
    let repo = state.user_repo();
    let user = match repo.find_by_email(&payload.email).await {
        Ok(Some(u)) if !u.is_email_verified() => u,
        Ok(Some(_)) | Ok(None) => {
            return (StatusCode::OK, Json(serde_json::json!({
                "message": "If this email is registered and not verified, a new code will be sent."
            }))).into_response();
        }
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json("Database error")).into_response();
        }
    };
    
    // Генерируем НОВЫЙ код (старый автоматически заменится в Redis)
    if let Some(ref code_service) = state.verification_code {
        match code_service.create_code(&payload.email).await {
            Ok(code) => {
                // Отправляем новый код через email-сервис
                if let Some(ref email_service) = state.email {
                    if let Err(e) = email_service
                        .send_verification_code(&payload.email, &user.username, &code)
                        .await
                    {
                        tracing::warn!("Failed to send verification code: {}", e);
                    }
                }
                
                // ✅ Устанавливаем кулдаун на 60 секунд
                set_resend_cooldown(&state, &payload.email).await;
                
                info!(email = %payload.email, "Verification code resent");
                (StatusCode::OK, Json(serde_json::json!({
                    "message": "A new verification code has been sent to your email."
                }))).into_response()
            }
            Err(e) => {
                tracing::error!("Failed to create verification code: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, Json("Failed to resend code")).into_response()
            }
        }
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json("Verification service not configured")).into_response()
    }
}

// ===== PROTECTED HANDLERS (без изменений) =====

#[utoipa::path(get, path = "/user/profile", tag = "user", security(("bearer_auth" = [])), responses((status = 200), (status = 401)))]
pub async fn get_profile_handler(
    VerifiedUser(username): VerifiedUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let repo = state.user_repo();
    match repo.find_by_username(&username).await {
        Ok(Some(user)) => (StatusCode::OK, Json(serde_json::json!({
            "username": user.username, 
            "created_at": user.created_at, 
            "is_verified": user.is_verified,
        }))).into_response(),
        _ => (StatusCode::NOT_FOUND, Json("User not found")).into_response(),
    }
}

#[utoipa::path(put, path = "/user/profile", tag = "user", security(("bearer_auth" = [])), request_body = UpdateProfileRequest, responses((status = 200, body = UpdateProfileResponse), (status = 401), (status = 404)))]
pub async fn update_profile_handler(
    VerifiedUser(username): VerifiedUser,
    State(state): State<AppState>,
    Json(payload): Json<UpdateProfileRequest>,
) -> impl IntoResponse {
    let repo = state.user_repo();
    let user = match repo.find_by_username(&username).await {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::NOT_FOUND, Json("User not found")).into_response(),
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json("Database error")).into_response();
        }
    };
    
    let update_data = crate::domain::UpdateUser {
        email: None,
        is_active: None,
        is_verified: payload.is_verified,
    };
    
    match repo.update(user.id, update_data).await {
        Ok(updated_user) => (StatusCode::OK, Json(UpdateProfileResponse {
            username: updated_user.username,
            is_verified: updated_user.is_verified,
            updated_at: updated_user.updated_at,
        })).into_response(),
        Err(e) => {
            tracing::error!("Failed to update profile: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json("Failed to update profile")).into_response()
        }
    }
}

#[utoipa::path(put, path = "/user/settings", tag = "user", security(("bearer_auth" = [])), request_body = serde_json::Value, responses((status = 200), (status = 401)))]
pub async fn update_settings_handler(
    VerifiedUser(username): VerifiedUser,
    _state: State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> impl IntoResponse {
    info!(username = %username, "Settings updated: {:?}", payload);
    (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response()
}

#[utoipa::path(post, path = "/auth/revoke-all", tag = "auth", security(("bearer_auth" = [])), responses((status = 200), (status = 401)))]
pub async fn revoke_all_sessions_handler(
    VerifiedUser(username): VerifiedUser,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let repo = state.user_repo();
    let user = match repo.find_by_username(&username).await {
        Ok(Some(u)) => u,
        Ok(None) => return (StatusCode::NOT_FOUND, Json("User not found")).into_response(),
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json("Database error")).into_response();
        }
    };
    
    let refresh_repo = crate::domain::RefreshTokenRepository::new(&state.db);
    if let Err(e) = refresh_repo.revoke_all_for_user(user.id).await {
        tracing::error!("Failed to revoke sessions: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json("Failed to revoke sessions")).into_response();
    }
    
    tracing::info!(username = %username, "All sessions revoked");
    (StatusCode::OK, Json(serde_json::json!({"message": "All sessions have been revoked"}))).into_response()
}

#[utoipa::path(post, path = "/auth/refresh", tag = "auth", request_body = RefreshRequest, responses((status = 200, body = RefreshResponse), (status = 401), (status = 403)))]
pub async fn refresh_handler(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> impl IntoResponse {
    use crate::domain::{RefreshTokenRepository, RefreshToken, CreateRefreshToken};
    
    let repo = RefreshTokenRepository::new(&state.db);
    let token_hash = RefreshToken::hash_token(&payload.refresh_token);
    
    let refresh = match repo.find_by_hash(&token_hash).await {
        Ok(Some(t)) => t,
        Ok(None) => return (StatusCode::UNAUTHORIZED, Json("Invalid refresh token")).into_response(),
        Err(e) => {
            tracing::error!("Database error: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json("Database error")).into_response();
        }
    };
    
    if !refresh.is_valid() {
        return (if refresh.is_revoked { StatusCode::FORBIDDEN } else { StatusCode::UNAUTHORIZED },
            Json(if refresh.is_revoked { "Token revoked" } else { "Token expired" })).into_response();
    }
    
    let user_repo = state.user_repo();
    let user = match user_repo.find_by_id(refresh.user_id).await {
        Ok(Some(u)) => u,
        _ => return (StatusCode::UNAUTHORIZED, Json("User not found")).into_response(),
    };
    
    let new_access = match state.jwt.generate_token(&user.username) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Failed to generate access token: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json("Token generation failed")).into_response();
        }
    };
    
    let new_refresh_token = RefreshToken::generate();
    let new_refresh = CreateRefreshToken {
        user_id: user.id,
        token: new_refresh_token.clone(),
        user_agent: refresh.user_agent.clone(),
        ip_address: refresh.ip_address.clone(),
        expires_in_days: 30,
    };
    
    if let Err(e) = repo.create(new_refresh).await {
        tracing::error!("Failed to create new refresh token: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, Json("Failed to refresh")).into_response();
    }
    
    let _ = repo.revoke(refresh.id).await;
    let _ = repo.mark_used(refresh.id).await;
    
    tracing::info!(username = %user.username, "Tokens refreshed");
    (StatusCode::OK, Json(RefreshResponse { access_token: new_access, refresh_token: new_refresh_token })).into_response()
}

#[utoipa::path(post, path = "/auth/logout", tag = "auth", security(("bearer_auth" = [])), request_body = RefreshRequest, responses((status = 200), (status = 401)))]
pub async fn logout_handler(
    _user: VerifiedUser,
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> impl IntoResponse {
    use crate::domain::{RefreshTokenRepository, RefreshToken};
    
    let repo = RefreshTokenRepository::new(&state.db);
    let token_hash = RefreshToken::hash_token(&payload.refresh_token);
    
    if let Ok(Some(token)) = repo.find_by_hash(&token_hash).await {
        let _ = repo.revoke(token.id).await;
        tracing::info!("User logged out, token revoked");
    }
    StatusCode::OK.into_response()
}

// ===== CAPTCHA CHECK =====

async fn check_captcha(state: &AppState, cf_turnstile_response: Option<&str>) -> Result<(), axum::http::StatusCode> {
    let Some(captcha) = &state.captcha else { return Ok(()) };
    let token = cf_turnstile_response.ok_or(axum::http::StatusCode::BAD_REQUEST)?;
    if !captcha.verify(token, None).await.map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)? {
        return Err(axum::http::StatusCode::BAD_REQUEST);
    }
    Ok(())
}