-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

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
