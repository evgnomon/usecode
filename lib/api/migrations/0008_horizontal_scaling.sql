-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- shard mappings, user placement, task assignee
--
-- Splits rows across several PostgreSQL instances and several named API
-- instances. Superseded by 0009 (computed placement), kept so that every
-- deployment walks the same revision history.

CREATE TABLE shard_mappings (
    partition_key VARCHAR NOT NULL,
    host VARCHAR NOT NULL,
    port INTEGER NOT NULL,
    database VARCHAR NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (partition_key)
);

-- Where a user's partitioned rows live; null means the main instance.
ALTER TABLE users ADD COLUMN partition_key VARCHAR;
CREATE INDEX ix_users_partition_key ON users (partition_key);

-- Which API instance owns a task; null means the main/first one.
ALTER TABLE tasks ADD COLUMN assignee VARCHAR;
CREATE INDEX ix_tasks_assignee ON tasks (assignee);

ALTER TABLE provider_credentials DROP CONSTRAINT IF EXISTS provider_credentials_user_id_fkey;
ALTER TABLE servers DROP CONSTRAINT IF EXISTS servers_user_id_fkey;
ALTER TABLE tasks DROP CONSTRAINT IF EXISTS tasks_user_id_fkey;
