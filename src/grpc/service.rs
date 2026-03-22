//! gRPC реализация AuthService для внутренних микросервисов

use azimuth_proto::azimuth::auth::v1::{
    auth_service_server::AuthService,
    VerifyTokenRequest, VerifyTokenResponse,
    GetUserRequest, GetUserResponse, get_user_request::Identifier,
    HealthRequest, HealthResponse,
};
use crate::domain::AppState;
use tonic::{Request, Response, Status};
use tracing::{info, warn};

/// Реализация gRPC сервиса аутентификации
#[derive(Clone)]
pub struct GrpcAuthService {
    state: AppState,
}

impl GrpcAuthService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl AuthService for GrpcAuthService {
    async fn verify_token(
        &self,
        request: Request<VerifyTokenRequest>,
    ) -> Result<Response<VerifyTokenResponse>, Status> {
        let req = request.into_inner();
        
        info!(
            service = %req.service_name,
            "VerifyToken request received"
        );
        
        // ✅ Просто валидируем JWT — без blacklist пока
        match self.state.jwt.verify_token(&req.token) {
            Ok(claims) => {
                info!(
                    username = %claims.sub,
                    service = %req.service_name,
                    "Token verified successfully"
                );
                
                Ok(Response::new(VerifyTokenResponse {
                    valid: true,
                    username: Some(claims.sub),
                    user_id: None,
                    expires_at: Some(claims.exp),
                    scope: req.required_scope,
                    error_message: String::new(),
                }))
            }
            Err(e) => {
                warn!(
                    service = %req.service_name,
                    error = %e,
                    "Token verification failed"
                );
                
                Ok(Response::new(VerifyTokenResponse {
                    valid: false,
                    username: None,
                    user_id: None,
                    expires_at: None,
                    scope: None,
                    error_message: e.to_string(),
                }))
            }
        }
    }

    async fn get_user(
        &self,
        request: Request<GetUserRequest>,
    ) -> Result<Response<GetUserResponse>, Status> {
        let req = request.into_inner();
        
        let username = match req.identifier {
            Some(Identifier::Username(name)) => name,
            Some(Identifier::UserId(id)) => {
                let repo = self.state.user_repo();
                match repo.find_by_id(id).await {
                    Ok(Some(user)) => user.username,
                    Ok(None) => {
                        return Ok(Response::new(GetUserResponse {
                            found: false,
                            user_id: None,
                            username: None,
                            quota_bytes: None,
                            used_bytes: None,
                            created_at: None,
                        }));
                    }
                    Err(e) => {
                        warn!("Database error: {}", e);
                        return Err(Status::internal("Database error"));
                    }
                }
            }
            None => {
                return Err(Status::invalid_argument("Identifier is required"));
            }
        };
        
        let repo = self.state.user_repo();
        match repo.find_by_username(&username).await {
            Ok(Some(user)) => {
                info!(username = %user.username, "User found");
                
                Ok(Response::new(GetUserResponse {
                    found: true,
                    user_id: Some(user.id),
                    username: Some(user.username),
                    quota_bytes: None,
                    used_bytes: None,
                    created_at: Some(user.created_at.timestamp()),
                }))
            }
            Ok(None) => {
                warn!(username = %username, "User not found");
                
                Ok(Response::new(GetUserResponse {
                    found: false,
                    user_id: None,
                    username: None,
                    quota_bytes: None,
                    used_bytes: None,
                    created_at: None,
                }))
            }
            Err(e) => {
                warn!("Database error: {}", e);
                Err(Status::internal("Database error"))
            }
        }
    }

    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        Ok(Response::new(HealthResponse {
            serving: true,
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: 0,
        }))
    }
}