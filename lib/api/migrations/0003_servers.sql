-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- servers and server type mappings

CREATE TABLE server_type_mappings (
    series VARCHAR NOT NULL,
    provider VARCHAR NOT NULL,
    provider_server_type VARCHAR NOT NULL,
    PRIMARY KEY (series)
);
INSERT INTO server_type_mappings (series, provider, provider_server_type) VALUES
    ('x1', 'hetzner', 'cx22'),
    ('x2', 'hetzner', 'cx32'),
    ('x4', 'hetzner', 'cx42'),
    ('x8', 'hetzner', 'cx52'),
    ('y1', 'digitalocean', 's-1vcpu-1gb'),
    ('y2', 'digitalocean', 's-2vcpu-2gb'),
    ('y4', 'digitalocean', 's-4vcpu-8gb'),
    ('y8', 'digitalocean', 's-8vcpu-16gb');

CREATE TABLE servers (
    id UUID NOT NULL,
    user_id UUID NOT NULL,
    provider VARCHAR NOT NULL,
    provider_server_id VARCHAR NOT NULL,
    type VARCHAR NOT NULL,
    name VARCHAR NOT NULL,
    status VARCHAR NOT NULL,
    public_ip4 VARCHAR,
    public_ip6 VARCHAR,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL,
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL,
    PRIMARY KEY (id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT uq_servers_provider_id UNIQUE (provider, provider_server_id)
);
CREATE INDEX ix_servers_user_id ON servers (user_id);
CREATE INDEX ix_servers_provider_server_id ON servers (provider_server_id);
