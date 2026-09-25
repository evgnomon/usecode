-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- location mappings and provider catalog

CREATE TABLE location_mappings (
    code VARCHAR NOT NULL,
    provider VARCHAR NOT NULL,
    provider_location_code VARCHAR NOT NULL,
    PRIMARY KEY (code)
);

CREATE TABLE provider_resources (
    id UUID NOT NULL,
    provider VARCHAR NOT NULL,
    kind VARCHAR NOT NULL,
    code VARCHAR NOT NULL,
    data JSON NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT uq_provider_resources_kind_code UNIQUE (provider, kind, code)
);
CREATE INDEX ix_provider_resources_provider ON provider_resources (provider);
CREATE INDEX ix_provider_resources_kind ON provider_resources (kind);
