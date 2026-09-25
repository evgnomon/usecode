-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- api_keys hash their own key
--
-- An API key arrives with nothing else, so the key hash is the only address
-- it can have: api_keys becomes a root table partitioned on key_hash.
-- api_keys.user_id and web_sessions.api_key_hash become plain columns;
-- user_api_keys (which lives with the user) is the reverse index.

ALTER TABLE web_sessions ADD COLUMN api_key_hash VARCHAR;
UPDATE web_sessions SET api_key_hash = api_keys.key_hash
    FROM api_keys WHERE api_keys.id = web_sessions.api_key_id;
-- A session whose key row is missing could not be logged out of anyway.
DELETE FROM web_sessions WHERE api_key_hash IS NULL;
ALTER TABLE web_sessions ALTER COLUMN api_key_hash SET NOT NULL;
ALTER TABLE web_sessions DROP CONSTRAINT IF EXISTS web_sessions_api_key_id_fkey;
ALTER TABLE web_sessions DROP COLUMN api_key_id;

CREATE TABLE user_api_keys (
    user_id UUID NOT NULL,
    key_hash VARCHAR NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (user_id, key_hash),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
INSERT INTO user_api_keys (user_id, key_hash, created_at)
    SELECT user_id, key_hash, created_at FROM api_keys
    ON CONFLICT DO NOTHING;

ALTER TABLE api_keys DROP CONSTRAINT IF EXISTS api_keys_user_id_fkey;
ALTER TABLE api_keys DROP CONSTRAINT IF EXISTS api_keys_key_hash_key;
ALTER TABLE api_keys DROP CONSTRAINT api_keys_pkey;
ALTER TABLE api_keys ADD CONSTRAINT api_keys_pkey PRIMARY KEY (key_hash);
ALTER TABLE api_keys DROP COLUMN id;
