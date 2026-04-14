# ensure_pod Role

Creates and manages Podman containers with standardized configuration and lifecycle management.

This role provides a comprehensive container deployment solution with automatic directory creation, configurable volumes, environment variables, and state management.

## Features

- Creates required working and configuration directories
- Deploys containers using Podman with customizable configuration
- Supports volume mounts, environment variables, and command overrides
- Configurable container state management (present/absent/started/stopped)
- Automatic restart policy configuration
- Certificate and security context support

## Default Variables

```yaml
z_container_full_name: "{{ z_user }}-{{ z_node_type }}-{{ z_container_name }}-{{ z_suffix }}"
z_container_user: "{{ z_user }}"
z_container_working_dir: "{{ z_backup_dir }}/{{ z_container_name }}/var"
z_container_config_dir: "{{ z_backup_dir }}/{{ z_container_name }}/etc"
z_container_volumes: []
z_container_command: ""
z_container_restart_policy: "unless-stopped"
z_container_user_in_container: "root"
z_container_state: "started"
```

## Usage

### Basic Container Deployment
```yaml
- hosts: all
  roles:
    - role: ensure_pod
      vars:
        z_container_name: "myapp"
        z_container_image: "docker.io/nginx:latest"
```

### Advanced Configuration
```yaml
- hosts: all
  roles:
    - role: ensure_pod
      vars:
        z_container_name: "webapp"
        z_container_image: "docker.io/myapp:v1.0"
        z_container_command: "/app/start.sh --config /etc/myapp/config.yml"
        z_container_volumes:
          - "/host/data:/container/data:rw"
          - "/host/config:/etc/myapp:ro"
        z_service_env_vars:
          APP_ENV: "production"
          LOG_LEVEL: "info"
        z_container_state: "started"
```

### Container Removal
```yaml
- hosts: all
  roles:
    - role: ensure_pod
      vars:
        z_container_name: "myapp"
        z_container_state: "absent"
```

## Directory Structure

The role automatically creates:
- `{{ z_container_working_dir }}` - Container working directory (mode 0700)
- `{{ z_container_config_dir }}` - Container configuration directory (mode 0700)

## Environment Variables

Default environment variables provided to containers:
- `Z_DOMAIN`: Domain extracted from inventory hostname
- `Z_HOST`: Full inventory hostname
- `CONTAINER_HOME`: Container working directory path
- `USE_DOMAIN_CERT`: Boolean for domain certificate usage

## Certificate Integration

The role supports certificate mounting with predefined paths:
- CA certificate: `{{ z_mount_cert_ca_file }}`
- Function certificate: `{{ z_mount_cert_func_pub }}`
- Function private key: `{{ z_mount_cert_func_key }}`
- PKCS#12 bundle: `{{ z_mount_cert_func_p12 }}`

## State Management

Container states supported:
- `started` - Container is running
- `present` - Container exists but may be stopped
- `absent` - Container is removed

## Dependencies

- Requires Podman container runtime
- Uses `containers.podman.podman_container` Ansible module
- Requires appropriate permissions for container management