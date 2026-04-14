# ensure_prometheus Role

Configures and deploys Prometheus monitoring server with TLS-secured metrics collection.

This role templates the Prometheus configuration file to scrape metrics from various targets including itself and Node Exporter instances, with secure TLS certificate authentication.

## Features

- Templates Prometheus configuration with customizable scrape intervals
- Configures secure HTTPS scraping with certificate authentication
- Pre-configured to monitor Prometheus itself and Node Exporter
- Supports custom retention policies and evaluation intervals
- Integrated with certificate management for secure monitoring

## Default Variables

```yaml
z_container_name: "prometheus"
z_container_image: docker.io/prom/prometheus:latest
prometheus_scrape_interval: 15s
prometheus_evaluation_interval: 15s
prometheus_retention_time: 15d
z_container_ports:
  - "9090:9090"
```

## Usage

### Basic Deployment
```yaml
- hosts: monitoring_servers
  roles:
    - role: ensure_prometheus
```

### Custom Configuration
```yaml
- hosts: monitoring_servers
  roles:
    - role: ensure_prometheus
      vars:
        prometheus_scrape_interval: "30s"
        prometheus_evaluation_interval: "30s"
        prometheus_retention_time: "30d"
```

## Configuration

The role creates a Prometheus configuration file at `{{ z_config_file }}` that includes:

### Global Settings
- Scrape interval: `{{ prometheus_scrape_interval }}`
- Evaluation interval: `{{ prometheus_evaluation_interval }}`

### Pre-configured Scrape Jobs

#### Prometheus Self-Monitoring
```yaml
- job_name: 'prometheus'
  static_configs:
    - targets: ['localhost:9090']
```

#### Node Exporter Monitoring
```yaml
- job_name: 'node-exporter'
  static_configs:
    - targets: ['{{ z_node_type }}-node-exporter.{{ z_domain }}:9100']
  scheme: https
  tls_config:
    ca_file: {{ z_mount_cert_ca_file }}
    cert_file: {{ z_mount_cert_func_pub }}
    key_file: {{ z_mount_cert_func_key }}
    insecure_skip_verify: false
```

## Container Configuration

Prometheus runs with the following command arguments:
- `--config.file={{ z_mount_config_file }}`
- `--storage.tsdb.path={{ z_mount_workdir }}/data`
- `--web.console.libraries=/etc/prometheus/console_libraries`
- `--web.console.templates=/etc/prometheus/consoles`
- `--web.enable-lifecycle`
- `--web.listen-address=0.0.0.0:9090`

## Volume Mounts

The container includes these volume mounts:
- Function certificates for TLS authentication
- CA certificate for verification
- Working directory for data storage
- Configuration directory (read-only)

## Web Interface

Prometheus web interface is accessible at:
- `http://hostname:9090` (if not using TLS)
- `https://hostname:9090` (with TLS configuration)

## Data Retention

Default retention period is `{{ prometheus_retention_time }}` (15 days), configurable via the `prometheus_retention_time` variable.

## Dependencies

- Requires certificates to be deployed via certificate management roles
- Depends on Node Exporter targets being properly configured with TLS
- Requires network connectivity to monitoring targets