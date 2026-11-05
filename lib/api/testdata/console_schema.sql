-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- The console's own schema before it used the API (lib/console/migrations
-- 0001 to 0003), for the import-console test.

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

-- A code mailed to finish a sign up or a log in. The browser holds the token
-- and the person the code; only their SHA-256 are kept. A sign up's account
-- doesn't exist yet, so its email and password hash wait here until the
-- code is right.
CREATE TABLE email_codes (
    token_hash    bytea PRIMARY KEY,
    purpose       text NOT NULL CHECK (purpose IN ('signup', 'login')),
    email         text NOT NULL,
    user_id       bigint REFERENCES users (id) ON DELETE CASCADE,
    password_hash text,
    code_hash     bytea NOT NULL,
    attempts      integer NOT NULL DEFAULT 0,
    expires_at    timestamptz NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now(),
    CHECK ((purpose = 'login') = (user_id IS NOT NULL)),
    CHECK ((purpose = 'signup') = (password_hash IS NOT NULL))
);
CREATE INDEX email_codes_expires_at ON email_codes (expires_at);

-- A code can also reset a forgotten password, carrying the hash of the new
-- one until the code is right. A reset for an address without an account
-- has no user: its code is never mailed, so it only ever runs out.
ALTER TABLE email_codes
    DROP CONSTRAINT email_codes_purpose_check,
    DROP CONSTRAINT email_codes_check,
    DROP CONSTRAINT email_codes_check1,
    ADD CONSTRAINT email_codes_purpose_check CHECK (purpose IN ('signup', 'login', 'reset')),
    ADD CONSTRAINT email_codes_signup_user_check CHECK (purpose <> 'signup' OR user_id IS NULL),
    ADD CONSTRAINT email_codes_login_user_check CHECK (purpose <> 'login' OR user_id IS NOT NULL),
    ADD CONSTRAINT email_codes_password_check CHECK ((purpose = 'login') = (password_hash IS NULL));
