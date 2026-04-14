# ensure_volume Role

Manages filesystem volume mounting and configuration for attached storage devices.

This role automatically creates mount points, configures `/etc/fstab` entries, and mounts volumes for attached storage devices with configurable filesystem types.

## Features

- Creates mount points under `/mnt/` for each attached volume
- Updates `/etc/fstab` with persistent mount configuration
- Mounts all configured volumes automatically
- Supports configurable filesystem types per volume
- Idempotent operations with proper error handling

## Default Variables

```yaml
default_filesystem_type: ext4
```

## Required Variables

```yaml
attached_volumes:
  volume_name:
    device_path: "/dev/sdb1"
    filesystem: "ext4"  # Optional, defaults to default_filesystem_type
  another_volume:
    device_path: "/dev/sdc1"
    filesystem: "xfs"
```

## Usage

### Basic Volume Mounting
```yaml
- hosts: storage_nodes
  roles:
    - role: ensure_volume
      vars:
        attached_volumes:
          data:
            device_path: "/dev/sdb1"
          logs:
            device_path: "/dev/sdc1"
            filesystem: "xfs"
```

### Multiple Volumes with Different Filesystems
```yaml
- hosts: all
  roles:
    - role: ensure_volume
      vars:
        default_filesystem_type: "xfs"
        attached_volumes:
          postgres_data:
            device_path: "/dev/sdb1"
            filesystem: "ext4"
          application_logs:
            device_path: "/dev/sdc1"
            # Will use default_filesystem_type (xfs)
          backup_storage:
            device_path: "/dev/sdd1"
            filesystem: "ext4"
```

## Mount Points

Volumes are mounted at `/mnt/{volume_name}`:
- Volume `data` → `/mnt/data`
- Volume `logs` → `/mnt/logs`
- Volume `postgres_data` → `/mnt/postgres_data`

## fstab Configuration

The role adds entries to `/etc/fstab` with the format:
```
{device_path} /mnt/{volume_name} {filesystem} defaults 0 2
```

Example:
```
/dev/sdb1 /mnt/data ext4 defaults 0 2
/dev/sdc1 /mnt/logs xfs defaults 0 2
```

## Filesystem Support

Supported filesystem types include:
- `ext4` (default)
- `xfs`
- `btrfs`
- `ext3`
- Any filesystem supported by the target system

## Error Handling

The role uses `failed_when: false` for the mount operation to prevent failures if volumes are already mounted or if there are temporary mount issues.

## Dependencies

- Requires root privileges for mount operations and `/etc/fstab` modification
- Target devices must exist and be properly formatted
- Filesystem utilities for the specified filesystem types must be installed