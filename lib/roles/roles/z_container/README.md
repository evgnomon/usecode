<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# z_container

Builds the workspace's `Dockerfile` as `ghcr.io/<owner>/<repo>:<track>` and
pushes it, or, on a branch delete event, deletes that tag from the registry.
Both steps run `uc ghcr` (from `lib/uc`), which must be on the `PATH`; the
token is `z_user_token`.

| Variable | Default | |
|---|---|---|
| `z_container_push` | `true` | push after building |
