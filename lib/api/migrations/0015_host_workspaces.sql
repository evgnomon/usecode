-- License-Identifier: HGL
-- Copyright (C) The Usecode Authors (see AUTHORS)

-- hosts: the workspaces a controller sees, and where its shell relay is
--
-- A host that is a Kubernetes controller reports the workspaces (`uc ws`)
-- of its cluster with each heartbeat and over its relay socket, with the
-- cluster's id (the uid of kube-system), so the controllers of one cluster
-- list each workspace once. `relay_url` is the API instance the host's
-- relay socket is open to, for the shells: the others hand a shell there.

ALTER TABLE hosts ADD COLUMN workspaces JSONB NOT NULL DEFAULT '[]';
ALTER TABLE hosts ADD COLUMN cluster_id VARCHAR NOT NULL DEFAULT '';
ALTER TABLE hosts ADD COLUMN relay_url VARCHAR;
