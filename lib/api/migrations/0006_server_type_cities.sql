-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- server type mapping cities

ALTER TABLE server_type_mappings ADD COLUMN cities JSON DEFAULT '[]' NOT NULL;
ALTER TABLE server_type_mappings ALTER COLUMN cities DROP DEFAULT;
