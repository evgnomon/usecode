```
                    ██████████████████████████████████████████████████████
                ████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░████
              ██░░                                                          ░░██
            ██░░    ███████╗  █████╗  ███╗   ███╗ ██╗ ███╗   ██╗              ░░██
          ██░░      ╚══███╔╝ ██╔══██╗ ████╗ ████║ ██║ ████╗  ██║                ░░██
        ██░░          ███╔╝  ███████║ ██╔████╔██║ ██║ ██╔██╗ ██║                  ░░██
        ██░░         ███╔╝   ██╔══██║ ██║╚██╔╝██║ ██║ ██║╚██╗██║                  ░░██
        ██░░        ███████╗ ██║  ██║ ██║ ╚═╝ ██║ ██║ ██║ ╚████║                  ░░██
        ██░░        ╚══════╝ ╚═╝  ╚═╝ ╚═╝     ╚═╝ ╚═╝ ╚═╝  ╚═══╝                  ░░██
          ██░░                                                                  ░░██
            ██░░      ┌─────────────────────────────────────────────────┐     ░░██
              ██░░    │  Build the foundation. Create the ground.       │   ░░██
                ████░░└─────────────────────────────────────────────────┘░░████
                    ██████████████████████████████████████████████████████

                         ╔═══════════════════════════════════════╗
                         ║  QEMU/KVM • Ansible • HGL/Blueprint   ║
                         ╚═══════════════════════════════════════╝
```

# Zamin

Zamin is a Linux and an identity for the client machine.

## Prerequisites

- [Packer](https://www.packer.io/) >= 1.1.0
- [QEMU/KVM](https://www.qemu.org/) with KVM support
- [Ansible](https://www.ansible.com/)
- Debian 13 base image at `~/.cache/blueprint/debian/linux_amd64/debian_13/debian_13.bin`

## Quick Start

1. Initialize Packer plugins:
   ```bash
   packer init zamin.pkr.hcl
   ```

2. Build the VM image:
   ```bash
   packer build zamin.pkr.hcl
   ```

The resulting QCOW2 image will be available at `./output-zamin/zamin`.

## Configuration

| Variable | Default | Description |
|----------|---------|-------------|
| `vm_name` | `zamin` | Name of the output VM |
| `cpu_cores` | `2` | Number of CPU cores |
| `memory` | `1024` | Memory in MB |
| `disk_size` | `5G` | Disk size |

Override variables during build:
```bash
packer build -var 'cpu_cores=4' -var 'memory=2048' zamin.pkr.hcl
```

## Provisioning

The VM is provisioned using Ansible via `main.yaml`. By default, it installs Nginx on Debian-based systems.

## License

See the main project repository for license information.
