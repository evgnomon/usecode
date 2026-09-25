-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- virtual shards: user-id placement, user directory, real user foreign keys
--
-- A user id hashes into one of 65536 virtual shards; shard_ranges maps a
-- contiguous run of those buckets to the instance holding them. Because
-- every user-owned table is keyed by the user id, they all hash to the same
-- instance as the users row — so the foreign keys 0008 had to drop come back
-- as real constraints. user_directory turns a phone number arriving at the
-- login endpoint into a user id.

CREATE TABLE shard_ranges (
    start_bucket SERIAL NOT NULL,
    end_bucket INTEGER NOT NULL,
    partition_key VARCHAR NOT NULL,
    host VARCHAR NOT NULL,
    port INTEGER NOT NULL,
    database VARCHAR NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (start_bucket)
);

CREATE TABLE user_directory (
    phone VARCHAR NOT NULL,
    user_id UUID NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (phone)
);
CREATE INDEX ix_user_directory_user_id ON user_directory (user_id);

-- Every user that exists today is on the database this migration is running
-- against, so its own users table is exactly the directory it needs.
INSERT INTO user_directory (phone, user_id, created_at)
    SELECT phone, id, created_at FROM users
    ON CONFLICT (phone) DO NOTHING;

DROP INDEX ix_users_partition_key;
ALTER TABLE users DROP COLUMN partition_key;
DROP TABLE shard_mappings;

ALTER TABLE provider_credentials ADD CONSTRAINT provider_credentials_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE;
ALTER TABLE servers ADD CONSTRAINT servers_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE;
ALTER TABLE tasks ADD CONSTRAINT tasks_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE;
