-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- shard_ranges: one map per table
--
-- Every row says whose map it belongs to, and every lookup names the map it
-- is reading. Existing rows are the users map; the other maps are seeded on
-- the next boot by Db::seed_shard_ranges.

ALTER TABLE shard_ranges ADD COLUMN "table" VARCHAR;
UPDATE shard_ranges SET "table" = 'users';
ALTER TABLE shard_ranges ALTER COLUMN "table" SET NOT NULL;

ALTER TABLE shard_ranges DROP CONSTRAINT shard_ranges_pkey;
ALTER TABLE shard_ranges ADD CONSTRAINT shard_ranges_pkey PRIMARY KEY ("table", start_bucket);
