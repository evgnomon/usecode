cuda-toolkit
============

Installs the NVIDIA CUDA Toolkit (`nvcc` and the CUDA libraries) from
NVIDIA's official apt/dnf repository, on a host that already has a
working NVIDIA driver.

Requirements
------------

- Debian/Ubuntu or RedHat/CentOS-family target.
- A working NVIDIA driver already installed (`nvidia-smi` must succeed).
  The driver itself is **not** managed by this role.
- Internet access on the target to reach `developer.download.nvidia.com`.

Role Variables
---------------

See `defaults/main.yml` for the full list. Notably:

- `cuda_toolkit_version` — package version suffix to pin, e.g. `"12-8"`
  for `cuda-toolkit-12-8`. Empty (default) installs the latest
  `cuda-toolkit` meta-package.
- `cuda_toolkit_distro_map` — maps OS family + version to the NVIDIA repo
  codename (e.g. `ubuntu2204`, `rhel9`). Add an entry if your target
  isn't covered.
- `cuda_toolkit_symlink` — where the versioned install is symlinked
  (default `/usr/local/cuda`).

What it does
------------

- Fails early if no NVIDIA driver is detected.
- Skips the install entirely if `nvcc` is already present at
  `{{ cuda_toolkit_symlink }}/bin/nvcc`.
- Adds NVIDIA's apt (`cuda-keyring`) or dnf repo and installs the
  toolkit package.
- Registers the toolkit's `lib64` directory with `ldconfig`.
- Adds `/etc/profile.d/cuda.sh` so interactive shells pick up `nvcc` and
  `LD_LIBRARY_PATH`.
- Verifies `nvcc --version` works before finishing.

Note for consuming roles
-------------------------

Ansible's `command`/`shell` modules don't source `/etc/profile.d`, so a
non-interactive task that needs `nvcc` (e.g. a CMake CUDA build) should
reference `{{ cuda_toolkit_symlink }}/bin` directly or add it to that
task's `environment.PATH`, rather than relying on the shell PATH.

Example Playbook
----------------

    - hosts: gpus
      become: true
      roles:
        - role: cuda-toolkit
        - role: llama-cpp
          vars:
            llama_cpp_backend: cuda
