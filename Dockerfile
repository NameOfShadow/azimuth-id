# =============================================================================
# BUILD STAGE
# =============================================================================
FROM rust:1.94-slim AS builder

WORKDIR /app

# Зависимости для компиляции
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    ca-certificates \
    openssl \
    wget \
    libssl3 \
    libpq5 \
    iputils-ping \
    && rm -rf /var/lib/apt/lists/*

# Копируем манифесты для кэширования зависимостей
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Пустые исходники для пре-компиляции зависимостей
RUN mkdir -p src && echo "fn main() {}" > src/main.rs
RUN cargo build --release --bin azimuth-id 2>/dev/null || true
RUN rm -f target/release/azimuth-id 2>/dev/null || true

# Копируем реальный исходный код и миграции
COPY src ./src
COPY crates/azimuth-proto/proto ./crates/azimuth-proto/proto
COPY migrations ./migrations

# Финальная сборка
RUN cargo build --release --bin azimuth-id

# =============================================================================
# RUNTIME STAGE
# =============================================================================
FROM debian:bookworm-slim

# Устанавливаем зависимости для динамически слинкованного бинарника
# ВАЖНО: \ должен быть последним символом на строке (без пробелов после!)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    openssl \
    wget \
    libssl3 \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 1000 -g root azimuth

WORKDIR /app

# Копируем бинарник ИЗ СТАДИИ builder (не из публичного образа!)
COPY --from=builder /app/target/release/azimuth-id /usr/local/bin/azimuth-id
COPY --from=builder /app/migrations /app/migrations

RUN chown -R azimuth:root /app
USER azimuth

ENV RUST_LOG=info
ENV SQLX_OFFLINE=false

EXPOSE 3000 50051

HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3000/health || exit 1

CMD ["/usr/local/bin/azimuth-id"]
