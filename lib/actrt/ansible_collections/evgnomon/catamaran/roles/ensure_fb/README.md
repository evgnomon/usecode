# ensure_fb Role

Configures and deploys Fluent Bit for log collection and forwarding to a centralized logging node.

This role templates the Fluent Bit configuration file to collect systemd journal logs and forward them securely via TLS to a designated logs node.

## Features

- Collects systemd journal logs from `/run/log/journal` and `/var/log/journal`
- Forwards logs securely using TLS with certificate authentication
- Configurable log forwarding destination
- Persistent database for journal read position tracking

## Default Variables

```yaml
z_container_name: "fb"
z_container_image: docker.io/fluent/fluent-bit:latest
logs_node_addr: logs-a.{{ z_domain }}
```

## Usage

```yaml
- hosts: all
  roles:
    - role: ensure_fb
      vars:
        logs_node_addr: "logs.example.com"  # Override default logs destination
```

## Configuration

The role creates a Fluent Bit configuration file at `{{ z_config_file }}` that:
- Reads from systemd journal with tail mode enabled
- Uses TLS encryption for secure log forwarding
- Authenticates using function-specific certificates
- Maintains a database for read position persistence

## Dependencies

- Requires certificates to be deployed via certificate management roles
- Depends on systemd journal availability
- Requires network connectivity to the logs node