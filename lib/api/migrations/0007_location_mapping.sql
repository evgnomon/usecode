-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- location mapping keyed per provider

ALTER TABLE location_mappings DROP CONSTRAINT location_mappings_pkey;
ALTER TABLE location_mappings ADD CONSTRAINT location_mappings_pkey PRIMARY KEY (code, provider);
