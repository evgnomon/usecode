-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- email accounts, mailed codes, sessions that run out, and billing
--
-- An account can be an email and a password now, next to a phone. The
-- console's sign up, log in and billing live here, so the console keeps no
-- database of its own.

ALTER TABLE users ALTER COLUMN phone DROP NOT NULL;
ALTER TABLE users ADD COLUMN email VARCHAR;
ALTER TABLE users ADD COLUMN password_hash VARCHAR;
CREATE UNIQUE INDEX ix_users_email ON users (email);

-- Email -> user id, as user_directory is for phones: at log in there is no
-- user id yet, so this is what turns the address into one. Hashed on the
-- email, so it is unique across every instance.
CREATE TABLE email_directory (
    email VARCHAR NOT NULL,
    user_id UUID NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (email)
);
CREATE INDEX ix_email_directory_user_id ON email_directory (user_id);

-- A code mailed to finish a sign up, a log in or a password reset. The
-- browser holds the token, the person the code; only their hashes are
-- kept. A sign up's account doesn't exist yet, so its email and password
-- hash wait here until the code is right. A reset for an address without
-- an account has no user: its code is never mailed, so it only runs out.
-- Hashed on the token, the only thing the code's check arrives with.
CREATE TABLE email_codes (
    token_hash VARCHAR NOT NULL,
    purpose VARCHAR NOT NULL CHECK (purpose IN ('signup', 'login', 'reset')),
    email VARCHAR NOT NULL,
    user_id UUID,
    password_hash VARCHAR,
    code_hash VARCHAR NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (token_hash),
    CHECK (purpose <> 'signup' OR user_id IS NULL),
    CHECK (purpose <> 'login' OR user_id IS NOT NULL),
    CHECK ((purpose = 'login') = (password_hash IS NULL))
);
CREATE INDEX ix_email_codes_expires_at ON email_codes (expires_at);

-- A browser's session is an API key that runs out.
ALTER TABLE api_keys ADD COLUMN expires_at TIMESTAMP WITH TIME ZONE;

-- One subscription per user, as Stripe last told us about it.
CREATE TABLE subscriptions (
    user_id UUID NOT NULL,
    stripe_customer_id VARCHAR NOT NULL,
    stripe_subscription_id VARCHAR,
    status VARCHAR NOT NULL DEFAULT 'incomplete',
    current_period_end TIMESTAMP WITH TIME ZONE,
    cancel_at_period_end BOOLEAN NOT NULL DEFAULT false,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (user_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- Stripe customer -> user id: a webhook names the customer, and this is how
-- it finds the user and so their instance.
CREATE TABLE stripe_customers (
    stripe_customer_id VARCHAR NOT NULL,
    user_id UUID NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (stripe_customer_id)
);

-- Webhook events already handled; Stripe delivers at least once.
CREATE TABLE stripe_events (
    id VARCHAR NOT NULL,
    received_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (id)
);
