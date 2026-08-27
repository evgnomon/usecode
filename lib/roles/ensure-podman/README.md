ensure-podman
=============

Installs Podman on the target host. Meant to be pulled in as a role
dependency (see `container-registry`) by anything that needs to run
containers via the `containers.podman` collection.

Role Variables
--------------

- `ensure_podman_packages` — list of packages to install (default:
  `[podman]`).

License
-------

MIT
