-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- People with an account. The email is kept lowercased.
CREATE TABLE users (
    id            bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    email         text NOT NULL UNIQUE,
    password_hash text NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);

-- A logged-in browser. Only the SHA-256 of the cookie's token is kept, so a
-- leaked table can't be used to log in.
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY,
    user_id    bigint NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_user_id ON sessions (user_id);
CREATE INDEX sessions_expires_at ON sessions (expires_at);

-- One supervisor subscription per user, as Stripe last told us about it.
CREATE TABLE subscriptions (
    user_id                bigint PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    stripe_customer_id     text NOT NULL UNIQUE,
    stripe_subscription_id text UNIQUE,
    status                 text NOT NULL DEFAULT 'incomplete',
    current_period_end     timestamptz,
    cancel_at_period_end   boolean NOT NULL DEFAULT false,
    updated_at             timestamptz NOT NULL DEFAULT now()
);

-- Stripe webhook events already handled; Stripe delivers at least once.
CREATE TABLE stripe_events (
    id          text PRIMARY KEY,
    received_at timestamptz NOT NULL DEFAULT now()
);
