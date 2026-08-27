container-registry
===================

Runs a password-protected OCI Distribution (`registry:2`) container under
Podman so images can be pushed/pulled with `podman login`.

Requirements
------------

- Podman is installed automatically via the `ensure-podman` role dependency.
- `containers.podman` and `community.general` collections installed on the
  control node (`ansible-galaxy collection install containers.podman community.general`).

Role Variables
--------------

See `defaults/main.yml`. Notably:

- `container_registry_username` / `container_registry_password` — basic-auth
  credentials for `podman login`. Set `container_registry_password` from a
  vaulted variable file; it defaults to empty and the role will fail if unset.
- `container_registry_port` — host port the registry is published on
  (default `5000`).
- `container_registry_data_dir` — where pushed image layers are stored on
  the host.

Generating the Password
------------------------

Generate a random password and store it as an `ansible-vault` encrypted
variable rather than committing it in plain text:

    openssl rand -base64 24

Then encrypt it into a group/host vars file, e.g.
`group_vars/registry/vault.yml`:

    ansible-vault encrypt_string 'the-generated-password' --name 'vault_container_registry_password'

Paste the output into `group_vars/registry/vault.yml`, and reference it from
`group_vars/registry/vars.yml`:

    container_registry_password: "{{ vault_container_registry_password }}"

The role itself does not generate a password — it fails with an assertion
error if `container_registry_password` is left empty.

Example Playbook
----------------

    - hosts: registry
      roles:
        - role: container-registry
          container_registry_username: registry
          container_registry_password: "{{ vault_container_registry_password }}"

After running, log in from a client with:

    podman login <host>:5000 -u registry

License
-------

MIT
