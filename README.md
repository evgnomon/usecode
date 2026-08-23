> No task that you do before using and/or developing AI for. Using and/or developing AI is also a task that we use and/or develop AI for. Soon would be no task that human do better than AI except AI. And we already have everything in this journey except that AI, `usecode`.

# Getting Started

### Development container

The repository includes a project-owned development image for VS Code Dev Containers.
Open the repository in VS Code and run **Dev Containers: Reopen in Container**.
The Dev Container is configured for nested Podman use; rebuild it after changing the
configuration with **Dev Containers: Rebuild Container Without Cache** so changes such
as the rootless `uidmap` dependency are installed.

For a standalone Docker or Podman session, build and start it with:

```bash
bash scripts/containerize.sh build
bash scripts/containerize.sh run
```

To build the standalone image that runs `play.sh` during the image build, select the
root Dockerfile explicitly:

```bash
bash scripts/containerize.sh build Dockerfile
```

The script prefers Podman when both runtimes are installed. Set `CONTAINER_RUNTIME=docker`
to select Docker explicitly, or set `USECODE_IMAGE` to use a different image tag. Rootless
Podman requires unprivileged user namespaces to be enabled by the host. If the workspace
itself is already inside a container, use the configured Dev Container or run the outer
container with equivalent privileged and AppArmor settings. Nested Podman uses the `vfs`
storage driver because OverlayFS cannot be mounted over the outer container's OverlayFS;
this is slower but works without additional host storage configuration. Inside this Dev
Container, the script uses rootful Podman through passwordless `sudo` to avoid nested
rootless namespace re-exec failures. Set `PODMAN_ROOTFUL=0` to explicitly request rootless
mode. Podman builds use the outer container's network namespace so package repositories
remain reachable when nested netavark DNS is unavailable.

Install `usecode` with a single command:

```bash
bash <(curl -fsSL https://raw.githubusercontent.com/evgnomon/usecode/refs/heads/master/play.sh)
```

Everything is ready in `/build` dir after build, you can run `usecode` with.

As it might take a long time to build `usecode` project, you can use save some time by using a cached build with:

```bash
git clone
cd usecode
make -DCACHED=1
sudo make install
```

Which gives the same `/build` dir with `usecode` ready to run, but with some time saved.

## Distribution
Files in `/dist` are made for distribution, make them using the following command if missing:

```bash
make dist
```
