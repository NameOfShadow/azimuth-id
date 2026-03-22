//! gRPC сервер модуль

// Импорт сервиса из подмодуля
mod service;
pub use service::GrpcAuthService;

// Импорт зависимостей
use crate::domain::AppState;
use azimuth_proto::azimuth::auth::v1::auth_service_server::AuthServiceServer;
use std::net::SocketAddr;
use tonic::transport::Server;
use tracing::info;

/// Запуск gRPC сервера
pub async fn serve(
    state: AppState, 
    addr: SocketAddr
) -> Result<(), String> {
    let auth_service = GrpcAuthService::new(state);
    
    info!("📡 Starting gRPC server on {}", addr);
    
    Server::builder()
        .add_service(AuthServiceServer::new(auth_service))
        .serve(addr)
        .await
        .map_err(|e| format!("gRPC serve failed: {}", e))?;
    
    Ok(())
}