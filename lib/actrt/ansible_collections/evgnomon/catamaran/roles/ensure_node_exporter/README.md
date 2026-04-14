# ensure_node_exporter Role

Deploys Prometheus Node Exporter as a containerized service for system metrics collection.

This role provides an empty task file that relies on inherited functionality from parent roles to deploy Node Exporter with appropriate container configuration and volume mounts.

## Features

- Deploys Prometheus Node Exporter container
- Provides host system metrics for monitoring
- Mounts host filesystem for comprehensive metric collection
- Configured for rootfs path monitoring

## Default Variables

```yaml
z_container_name: "node-exporter"
z_container_image: quay.io/prometheus/node-exporter:latest
z_container_command: "--path.rootfs=/host"
z_container_volumes:
  - "/:/host:ro,rslave"
```

## Usage

```yaml
- hosts: monitoring_nodes
  roles:
    - role: ensure_node_exporter
```

## Metrics Exposed

Node Exporter provides system-level metrics including:
- CPU usage and load averages
- Memory and swap usage
- Disk I/O and filesystem metrics
- Network interface statistics
- System uptime and boot time

## Port Access

Node Exporter typically listens on port `9100` and can be accessed at:
- `http://hostname:9100/metrics` (if not using TLS)
- `https://hostname:9100/metrics` (with TLS configuration)

## Dependencies

- Requires container runtime (Podman)
- Inherits configuration from parent container management roles
- May require certificate configuration for TLS-enabled metrics endpoint