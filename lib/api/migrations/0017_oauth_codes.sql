-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- OpenID Connect: the codes another site swaps for a login
--
-- When a site that logs in with usecode.dev (Forgejo) sends someone here
-- and they're logged in, they go back with a code; the site's server then
-- swaps it for tokens, once, within a couple of minutes. Only the code's
-- hash is kept. Hashed on it, the only thing the swap arrives with.
CREATE TABLE oauth_codes (
    code_hash VARCHAR NOT NULL,
    client_id VARCHAR NOT NULL,
    user_id UUID NOT NULL,
    redirect_uri VARCHAR NOT NULL,
    scope VARCHAR NOT NULL,
    nonce VARCHAR,
    code_challenge VARCHAR,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (code_hash)
);
CREATE INDEX ix_oauth_codes_expires_at ON oauth_codes (expires_at);
