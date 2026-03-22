#![warn(rust_2018_idioms)]

use std::net::SocketAddr;
use std::io::Write;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod config;
mod domain;
mod grpc;
mod http;

fn main() {
    // === Синхронная отладка ===
    eprintln!("=== AZIMUTH SYNC DEBUG START ===");
    let _ = std::io::stderr().flush();
    
    // Проверка переменных окружения
    for var in &["DATABASE_URL", "JWT_SECRET", "HTTP_ADDR"] {
        match std::env::var(var) {
            Ok(v) => eprintln!("✓ {}: SET (len={})", var, v.len()),
            Err(e) => eprintln!("✗ {}: MISSING ({})", var, e),
        }
    }
    let _ = std::io::stderr().flush();
    eprintln!("=== SYNC DEBUG END, creating Tokio runtime ===");
    
    // Создаём Tokio runtime
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_name("azimuth-worker")
        .build()
    {
        Ok(runtime) => {
            eprintln!("✓ Tokio runtime created successfully");
            let _ = std::io::stderr().flush();
            runtime
        }
        Err(e) => {
            eprintln!("✗ FATAL: Failed to create Tokio runtime: {}", e);
            std::process::exit(1);
        }
    };
    
    // Запускаем async_main
    eprintln!("=== Starting async_main ===");
    match rt.block_on(async_main()) {
        Ok(()) => {
            eprintln!("✓ async_main completed successfully");
        }
        Err(e) => {
            eprintln!("✗ FATAL: async_main error: {}", e);
            std::process::exit(1);
        }
    }
}

async fn async_main() -> Result<(), String> {
    // Инициализация логгера
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    tracing::info!("🧭 Azimuth-ID starting...");
    
    // Загрузка конфига
    let config = config::Config::load()
        .map_err(|e| format!("Config load failed: {}", e))?;
    
    tracing::info!("Config loaded: grpc={}, http={}", config.grpc_addr, config.http_addr);
    
    // Подключение к БД
    let db = sqlx::PgPool::connect(&config.database_url)
        .await
        .map_err(|e| format!("Database connect failed: {}", e))?;
    
    tracing::info!("Database connected");
    
    // Миграции
    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .map_err(|e| format!("Migrations failed: {}", e))?;
    
    tracing::info!("Migrations applied");
    
    // ===== СОЗДАНИЕ СОСТОЯНИЯ — ТЕПЕРЬ С .await =====
    let state = domain::AppState::new(db, config).await
        .map_err(|e| format!("AppState initialization failed: {}", e))?;
    
    let grpc_addr: SocketAddr = state.config.grpc_addr.parse()
        .map_err(|e| format!("Invalid gRPC address: {}", e))?;
    let http_addr: SocketAddr = state.config.http_addr.parse()
        .map_err(|e| format!("Invalid HTTP address: {}", e))?;
    
    let state_for_grpc = state.clone();
    let state_for_http = state.clone();
    
    // Запуск серверов
    let grpc_handle = tokio::spawn(async move {
        grpc::serve(state_for_grpc, grpc_addr).await
            .map_err(|e| format!("gRPC server error: {}", e))
    });
    
    let http_handle = tokio::spawn(async move {
        http::serve(state_for_http, http_addr).await
            .map_err(|e| format!("HTTP server error: {}", e))
    });
    
    tracing::info!("🚀 Azimuth-ID ready: gRPC on {}, HTTP on {}", grpc_addr, http_addr);
    
    // ===== Ждём завершения — ИСПРАВЛЕННАЯ ОБРАБОТКА ОШИБОК =====
    tokio::select! {
        res = grpc_handle => {
            match res {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(format!("gRPC server error: {}", e)),
                Err(e) => Err(format!("gRPC task panicked: {}", e)),
            }
        }
        res = http_handle => {
            match res {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(format!("HTTP server error: {}", e)),
                Err(e) => Err(format!("HTTP task panicked: {}", e)),
            }
        }
    }
}