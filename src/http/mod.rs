#![allow(dead_code)]

pub mod handlers;
pub mod middleware;

use crate::domain::AppState;
use axum::{middleware as axum_middleware, Router};
use std::net::SocketAddr;
use tower_http::cors::{CorsLayer, AllowOrigin};
use tracing::info;

pub async fn serve(state: AppState, addr: SocketAddr) -> Result<(), String> {
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST, axum::http::Method::DELETE, axum::http::Method::PUT])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]);

    let public_routes = Router::new()
        .route("/auth/register", axum::routing::post(handlers::register_handler))
        .route("/auth/login", axum::routing::post(handlers::login_handler))
        .route("/auth/verify-code", axum::routing::post(handlers::verify_code_handler))  // ← НОВЫЙ
        .route("/auth/resend-verification", axum::routing::post(handlers::resend_verification_handler))
        .route("/health", axum::routing::get(handlers::health_handler));

    let protected_routes = Router::new()
        .route("/user/profile", axum::routing::get(handlers::get_profile_handler))
        .route("/user/profile", axum::routing::put(handlers::update_profile_handler))
        .route("/user/settings", axum::routing::put(handlers::update_settings_handler))
        .route("/auth/logout", axum::routing::post(handlers::logout_handler))
        .route("/auth/revoke-all", axum::routing::post(handlers::revoke_all_sessions_handler))
        .layer(axum_middleware::from_fn_with_state(state.clone(), middleware::auth_middleware));

    let app = Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(cors)
        .with_state(state);

    info!("🌐 Starting HTTP server on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| format!("Failed to bind: {}", e))?;
    axum::serve(listener, app).await.map_err(|e| format!("HTTP serve failed: {}", e))?;
    Ok(())
}