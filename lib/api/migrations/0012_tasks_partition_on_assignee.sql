-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- tasks are partitioned on their assignee
--
-- A task is read by the node sweeping its own backlog, so tasks becomes a
-- root table partitioned on assignee, which joins the primary key and stops
-- being nullable. Before this runs, the migration runner hands rows with a
-- null assignee to USECODE_AGENT_MAIN_NODE_NAME (or USECODE_AGENT_NODE_NAME)
-- when set; whatever is still unassigned is unaddressable and is deleted.
-- user_tasks (which lives with the user) is the reverse index.

CREATE TABLE user_tasks (
    user_id UUID NOT NULL,
    task_id UUID NOT NULL,
    assignee VARCHAR NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (user_id, task_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

DELETE FROM tasks WHERE assignee IS NULL;
ALTER TABLE tasks ALTER COLUMN assignee SET NOT NULL;

INSERT INTO user_tasks (user_id, task_id, assignee, created_at)
    SELECT user_id, id, assignee, created_at FROM tasks
    ON CONFLICT DO NOTHING;

ALTER TABLE tasks DROP CONSTRAINT IF EXISTS tasks_user_id_fkey;
ALTER TABLE tasks DROP CONSTRAINT tasks_pkey;
-- Assignee first: a sweep reads one contiguous run of the key.
ALTER TABLE tasks ADD CONSTRAINT tasks_pkey PRIMARY KEY (assignee, id);
-- The primary key now leads with it, so the standalone index is dead weight.
DROP INDEX ix_tasks_assignee;
