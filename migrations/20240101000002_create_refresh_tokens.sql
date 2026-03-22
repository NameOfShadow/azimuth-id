-- Таблица refresh токенов
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    
    -- Хеш токена (не сам токен!)
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    
    -- Метаданные
    user_agent TEXT,
    ip_address INET,
    
    -- Временные метки
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_used_at TIMESTAMPTZ,
    
    -- Статус
    is_revoked BOOLEAN NOT NULL DEFAULT FALSE
);

-- Индексы
CREATE INDEX idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_token_hash ON refresh_tokens(token_hash);
CREATE INDEX idx_refresh_tokens_expires_at ON refresh_tokens(expires_at) WHERE NOT is_revoked;