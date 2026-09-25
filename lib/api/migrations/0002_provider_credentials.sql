-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- provider credentials

CREATE TABLE provider_credentials (
    user_id UUID NOT NULL,
    provider VARCHAR NOT NULL,
    credentials_encrypted VARCHAR NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (user_id, provider),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
