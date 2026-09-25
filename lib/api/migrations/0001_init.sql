-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- init schema

CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE users (
    id UUID NOT NULL,
    phone VARCHAR NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (id)
);
CREATE UNIQUE INDEX ix_users_phone ON users (phone);

CREATE TABLE otps (
    phone VARCHAR NOT NULL,
    code_hash VARCHAR NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    resend_after TIMESTAMP WITH TIME ZONE NOT NULL,
    attempts INTEGER NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (phone)
);

CREATE TABLE api_keys (
    id UUID NOT NULL,
    user_id UUID NOT NULL,
    key_hash VARCHAR NOT NULL,
    label VARCHAR NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    last_used_at TIMESTAMP WITH TIME ZONE,
    revoked_at TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY (id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    UNIQUE (key_hash)
);
CREATE INDEX ix_api_keys_user_id ON api_keys (user_id);

CREATE TABLE web_sessions (
    token_hash VARCHAR NOT NULL,
    user_id UUID NOT NULL,
    api_key_id UUID NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    revoked_at TIMESTAMP WITH TIME ZONE,
    PRIMARY KEY (token_hash),
    FOREIGN KEY (api_key_id) REFERENCES api_keys (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
CREATE INDEX ix_web_sessions_user_id ON web_sessions (user_id);
