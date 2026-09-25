<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

<p align="center">
  <img src="assets/logo.svg" alt="VM Logo" width="300">
</p>

# vm

A lightweight KVM/QEMU virtual machine creation tool written in Rust.

## Features

- Create and manage KVM/QEMU virtual machines
- Automatic cloud-init configuration with ISO generation
- IP address detection for running VMs
- Configurable VM specifications (memory, vCPUs, machine type)
- Support for custom base images
- Clean modular architecture

## Installation

### Dependencies

- Rust (stable) and a C compiler
- libvirt development libraries
- libisofs, libisoburn, libburn libraries

On Debian/Ubuntu:
```bash
sudo apt install build-essential libvirt-dev libisofs-dev libisoburn-dev libburn-dev
```

On Arch Linux:
```bash
sudo pacman -S base-devel libvirt libisofs libisoburn libburn
```

### Build

```bash
make build
sudo make install
```

Development tasks: `make test`, `make lint`, `make fmt`.

### Configuration

Create a configuration file at `/etc/vm/config` or `~/.config/vm/config`:

```
base_image_path=/usr/share/vm/images
vm_storage_path=/var/lib/libvirt/vm
cloud_init_template_path=/usr/share/vm/images/cloud-init
default_memory=1048576
default_vcpus=2
default_machine=pc-q35-10.0
max_retries=30
```

### Setting Up Base Images

Create the images directory and add your base VM image:

```bash
sudo mkdir -p /usr/share/vm/images
sudo mkdir -p /usr/share/vm/images/cloud-init
```

Download and extract ZAmin cloud image:
```bash
sudo wget -P /usr/share/vm/images/ \
  https://archive.evgnomon.org/zamin/zamin-0.0.1.tar.gz
sudo tar -xzf /usr/share/vm/images/zamin-0.0.1.tar.gz -C /usr/share/vm/images/
```


Create the cloud-init user-data template:
```bash
sudo tee /usr/share/vm/images/cloud-init/cloud-init-user-data.yaml << 'EOF'
#cloud-config
users:
  - name: user
    sudo: ALL=(ALL) NOPASSWD:ALL
    shell: /bin/bash
    ssh_authorized_keys:
      - ssh-rsa YOUR_PUBLIC_KEY_HERE
EOF
```

Replace `YOUR_PUBLIC_KEY_HERE` with your actual SSH public key from `~/.ssh/id_rsa.pub`.

## Usage

### Basic Usage

Create a new VM (legacy mode):
```bash
vm myvm
```

### Commands

#### Create a VM
```bash
vm create myvm
```

With custom specifications:
```bash
vm create myvm --memory 2GiB --vcpus 4
vm create myvm --machine pc-q35-9.0
vm create myvm --image /path/to/custom/image.qcow2
```

Create but don't start:
```bash
vm create myvm --no-start
```

Don't wait for IP address:
```bash
vm create myvm --no-wait-ip
```

#### List VMs
```bash
vm list
```

#### Show VM Information
```bash
vm info myvm
```

#### Start a VM
```bash
vm start myvm
```

#### Stop a VM
```bash
vm stop myvm
```

Force stop (poweroff):
```bash
vm stop myvm --force
```

#### Delete a VM
```bash
vm delete myvm
```

Force delete a running VM:
```bash
vm delete myvm --force
```

#### Get VM IP Address
```bash
vm ip myvm
```

### Global Options

- `--help, -h` - Show help message
- `--version, -v` - Show version information

### Examples

Create a web server VM:
```bash
vm create webserver --memory 2GiB --vcpus 2
vm ip webserver
```

Create multiple VMs:
```bash
vm create db1 --memory 4GiB --vcpus 4
vm create db2 --memory 4GiB --vcpus 4
vm create app1 --memory 1GiB --vcpus 2
```

Manage VMs:
```bash
vm list
vm info webserver
vm stop webserver
vm start webserver
```

## Architecture

The project is organized into modular components:

- `src/config.rs` - Configuration management
- `src/cloudinit.rs` - Cloud-init ISO generation
- `src/iso.c` - ISO image writing via libisofs/libisoburn
- `src/libvirt/` - Libvirt FFI bindings and safe wrappers
- `src/network.rs` - Network and IP address detection
- `src/ssh_conf.rs` - ssh_config.d host entries
- `src/vm.rs` - VM creation and management
- `src/wyhash.rs` - Stable hashing for MAC and instance-id derivation
- `src/main.rs` - CLI interface
- `src/lib.rs` - Library exports

## Library Usage

vm can also be used as a Rust library:

```rust
use vm::{Config, Connection, VmSpecs};

let conn = Connection::open("qemu:///system")?;

let cfg = Config::new();
let specs = VmSpecs {
    memory: 2 * 1024 * 1024, // 2GiB in KiB
    vcpus: 4,
    ..VmSpecs::default()
};

vm::create_vm(&conn, &cfg, "myvm", &specs)?;
```

## Troubleshooting

### Permission Denied

Make sure you have proper permissions to access libvirt:
```bash
sudo usermod -a -G libvirt $USER
sudo usermod -a -G kvm $USER
```

### VM Not Getting IP

Check that the default network is active:
```bash
virsh net-list --all
virsh net-start default
virsh net-autostart default
```

### Cloud-init Not Working

Ensure cloud-init is installed in your base image and the template file exists:
```bash
ls /usr/share/vm/images/cloud-init/cloud-init-user-data.yaml
```

## Contributing

Contributions are welcome! Please ensure your code follows the project's style and includes tests where appropriate.

## License

HGL General License - See COPYING file.
