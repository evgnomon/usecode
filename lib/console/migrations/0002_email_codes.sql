-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

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
