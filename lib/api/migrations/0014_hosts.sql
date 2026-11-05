-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- hosts: usecoded calling home
--
-- A host running usecoded registers itself with a one-time enrollment
-- token `uc daemon install` asked for with the user's API key, and from
-- then on says it's alive with a secret of its own. Both tokens are minted
-- with the owner's virtual shard in front, as web session tokens are, so
-- both tables live with the user and only their hashes are kept.

CREATE TABLE host_enrollments (
    token_hash VARCHAR NOT NULL,
    user_id UUID NOT NULL,
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (token_hash),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
CREATE INDEX ix_host_enrollments_user_id ON host_enrollments (user_id);

-- One row per machine and user: the same machine registering again (a
-- reinstall) gets its row back with a new secret.
CREATE TABLE hosts (
    id UUID NOT NULL,
    user_id UUID NOT NULL,
    name VARCHAR NOT NULL,
    machine_id VARCHAR NOT NULL,
    secret_hash VARCHAR NOT NULL,
    version VARCHAR NOT NULL DEFAULT '',
    arch VARCHAR NOT NULL DEFAULT '',
    os VARCHAR NOT NULL DEFAULT '',
    modules JSONB NOT NULL DEFAULT '[]',
    uptime_seconds BIGINT,
    last_ip VARCHAR,
    last_seen_at TIMESTAMP WITH TIME ZONE NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT uq_hosts_user_machine UNIQUE (user_id, machine_id)
);
CREATE UNIQUE INDEX ix_hosts_secret_hash ON hosts (secret_hash);
-- The console pages through a user's hosts by name.
CREATE INDEX ix_hosts_user_name ON hosts (user_id, name, id);
