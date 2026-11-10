-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- usernames
--
-- An account picks a username once: at sign up, or on the account page for
-- the accounts made before there were usernames. It never changes, so what
-- logs in elsewhere with it (Forgejo, through OpenID Connect) keeps meaning
-- the same person.

ALTER TABLE users ADD COLUMN username VARCHAR;

-- Username -> user id, as email_directory is for emails. It is what keeps a
-- name unique across every instance, so users has no index of its own on
-- it. Hashed on the username.
CREATE TABLE username_directory (
    username VARCHAR NOT NULL,
    user_id UUID NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (username)
);
CREATE INDEX ix_username_directory_user_id ON username_directory (user_id);

-- A sign up's username waits with its code, as its password hash does. A
-- code an older build started has none; that account picks one later.
ALTER TABLE email_codes ADD COLUMN username VARCHAR;
