-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- tasks

CREATE TABLE tasks (
    id UUID NOT NULL,
    user_id UUID NOT NULL,
    kind VARCHAR NOT NULL,
    state VARCHAR NOT NULL,
    resources JSON NOT NULL,
    payload JSON NOT NULL,
    error VARCHAR,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
CREATE INDEX ix_tasks_user_id ON tasks (user_id);
CREATE INDEX ix_tasks_kind ON tasks (kind);
