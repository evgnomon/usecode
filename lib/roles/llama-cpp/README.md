llama-cpp
=========

Builds and installs [llama.cpp](https://github.com/ggml-org/llama.cpp) from
source (CMake), and optionally runs a single-instance `llama-server` behind
a hardened systemd unit.

Requirements
------------

- Debian/Ubuntu or RedHat/CentOS-family target.
- Internet access on the target to clone the repo and (if
  `llama_cpp_server_hf_repo` is used) to download a model from Hugging Face.
- For `llama_cpp_backend: cuda`, the NVIDIA CUDA Toolkit must already be
  installed on the target — the role checks for `nvcc` at
  `llama_cpp_cuda_home` (default `/usr/local/cuda`) and fails early if
  it's missing, but does not install the toolkit itself. Run the
  `cuda-toolkit` role first to install it.

Role Variables
---------------

See `defaults/main.yml` for the full list. Notably:

- `llama_cpp_version` — git ref to build (default `master`; pin to a
  release tag such as `b6100` in production).
- `llama_cpp_backend` — `cpu` (default), `cuda`, `vulkan`, or `hip`.
- `llama_cpp_native` — build with `-DGGML_NATIVE=ON` (default `true`),
  tuned for the *build* host's CPU. Set to `false` if you build once and
  ship the binaries to other machines.
- `llama_cpp_prefix` — install prefix (default `/usr/local`).
- `llama_cpp_force_rebuild` — force a rebuild even if the repo didn't
  change (default `false`).

Server (optional, off by default):

- `llama_cpp_server_enabled` — set `true` to install and run `llama-server`
  as a systemd service.
- `llama_cpp_server_model` — absolute path to a local `.gguf` file, **or**
- `llama_cpp_server_hf_repo` — a Hugging Face repo to pull instead (e.g.
  `ggml-org/gemma-3-1b-it-GGUF`). Exactly one of these two must be set
  when the server is enabled.
- `llama_cpp_server_host` / `llama_cpp_server_port` — default
  `127.0.0.1:8080`.
- `llama_cpp_server_ctx_size` — context size (default `4096`).
- `llama_cpp_server_ngl` — number of layers offloaded to GPU (default `0`,
  i.e. CPU-only inference).
- `llama_cpp_server_extra_args` — list of extra `llama-server` CLI flags.
- `llama_cpp_server_api_keys` — list of bearer tokens for `--api-key-file`.
  An empty list means **no authentication at all**, which is only safe
  when bound to `127.0.0.1`. Set this from `ansible-vault`, never in a
  plain `group_vars` file.
- `llama_cpp_allow_open_listener` — set `true` only when something else
  (a reverse proxy, a firewall, a WireGuard interface) already restricts
  access to a non-local, unauthenticated listener. The role fails the
  play otherwise, to stop you from accidentally exposing an open
  inference endpoint.

Safety checks
-------------

- The play asserts `llama_cpp_server_model` or `llama_cpp_server_hf_repo`
  is set whenever `llama_cpp_server_enabled` is `true`.
- The play fails if the server would listen on a non-local address with
  no API keys configured and `llama_cpp_allow_open_listener` isn't set.
- `llama-server` runs as a dedicated, unprivileged `llama` system user
  under a hardened unit (`NoNewPrivileges`, `ProtectSystem=strict`,
  `ProtectHome=true`, `ReadWritePaths` scoped to its state dir only).
- The build only re-runs (`git clone` + `cmake --build` + `cmake
  --install`) when the checkout changed, the binary is missing, or
  `llama_cpp_force_rebuild` is `true` — repeated runs are fast no-ops.

Generating an API key
----------------------

Generate a random token and store it as an `ansible-vault` encrypted
variable rather than committing it in plain text:

    openssl rand -base64 32

Then encrypt it into a group/host vars file, e.g.
`group_vars/llama/vault.yml`:

    ansible-vault encrypt_string 'the-generated-token' --name 'vault_llama_cpp_api_key'

Paste the output into `group_vars/llama/vault.yml`, and reference it from
`group_vars/llama/vars.yml`:

    llama_cpp_server_api_keys:
      - "{{ vault_llama_cpp_api_key }}"

Example Playbook
----------------

Local, CPU-only server, listening only on `127.0.0.1` (no API key needed):

    - hosts: main
      become: true
      roles:
        - role: llama-cpp
          llama_cpp_server_enabled: true
          llama_cpp_server_hf_repo: ggml-org/gemma-3-1b-it-GGUF

Server reachable from other hosts, API-key protected:

    - hosts: main
      become: true
      vars_files:
        - vault.yaml
      roles:
        - role: llama-cpp
          llama_cpp_backend: cuda
          llama_cpp_server_enabled: true
          llama_cpp_server_model: /opt/models/my-model.gguf
          llama_cpp_server_host: 0.0.0.0
          llama_cpp_server_ngl: 999
          llama_cpp_server_api_keys:
            - "{{ vault_llama_cpp_api_key }}"

If the server should be reachable from other hosts, also run the
`firewall` role first with the server's port already present in
`firewall_allow_rules`, same as for `ssh-server`.

Using the running server
-------------------------

`llama-server` exposes an OpenAI-compatible API. Once the service is up:

    curl http://127.0.0.1:8080/v1/chat/completions \
      -H "Content-Type: application/json" \
      -H "Authorization: Bearer <token-from-llama_cpp_server_api_keys>" \
      -d '{
            "model": "local",
            "messages": [{"role": "user", "content": "Hello!"}]
          }'

Check status and logs on the target:

    systemctl status llama-server
    journalctl -u llama-server -f

Build-only install (no server)
-------------------------------

Leave `llama_cpp_server_enabled` at its default (`false`) to just build
and install the `llama-cli` / `llama-server` binaries under
`llama_cpp_prefix` (default `/usr/local`), without running any service:

    - hosts: main
      become: true
      roles:
        - role: llama-cpp

License
-------

MIT
