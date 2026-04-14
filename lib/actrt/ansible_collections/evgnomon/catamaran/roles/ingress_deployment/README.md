# Ansible Role: nginx_podman

Deploy nginx using Podman Kube Play, showcasing multiple Kubernetes resource types.

## Resources Created

| Resource | Description |
|----------|-------------|
| **ConfigMap** | Stores nginx.conf and index.html |
| **Secret** | Stores credentials (username, password, api-key) |
| **PersistentVolumeClaim** | Persistent storage for nginx logs |
| **Pod** | Standalone nginx pod with health probes |
| **Deployment** | Replicated nginx pods |
| **DaemonSet** | Nginx on every node |
| **Job** | One-time connectivity test |

## Requirements

- Podman installed on target host
- Ansible 2.10+

## Role Variables

### General Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `nginx_app_name` | `nginx-demo` | Application name prefix |
| `nginx_image` | `docker.io/library/nginx:alpine` | Nginx container image |
| `curl_image` | `docker.io/curlimages/curl:latest` | Curl image for job |

### Networking

| Variable | Default | Description |
|----------|---------|-------------|
| `nginx_container_port` | `80` | Container port |
| `nginx_host_port` | `8080` | Host port mapping |
| `nginx_worker_connections` | `1024` | Nginx worker connections |

### Resources

| Variable | Default | Description |
|----------|---------|-------------|
| `nginx_memory_limit` | `128Mi` | Memory limit |
| `nginx_cpu_limit` | `250m` | CPU limit |
| `nginx_memory_request` | `64Mi` | Memory request |
| `nginx_cpu_request` | `100m` | CPU request |

### Storage

| Variable | Default | Description |
|----------|---------|-------------|
| `nginx_pvc_storage_size` | `100Mi` | PVC storage size |
| `nginx_pvc_access_mode` | `ReadWriteOnce` | PVC access mode |

### Deployment

| Variable | Default | Description |
|----------|---------|-------------|
| `nginx_deployment_replicas` | `2` | Number of replicas |

### Secrets (Override in vault!)

| Variable | Default | Description |
|----------|---------|-------------|
| `nginx_secret_username` | `admin` | Username |
| `nginx_secret_password` | `supersecret123` | Password |
| `nginx_secret_api_key` | `my-api-key-12345` | API key |

### Podman Settings

| Variable | Default | Description |
|----------|---------|-------------|
| `nginx_kube_manifest_dest` | `/tmp/nginx-kube-manifest.yaml` | Manifest path |
| `nginx_podman_network` | `""` | Podman network (empty = default) |
| `nginx_auto_start` | `true` | Auto-start pods |

## Example Playbook

```yaml
---
- name: Deploy nginx with Podman
  hosts: servers
  become: true
  
  vars:
    nginx_host_port: 8080
    nginx_deployment_replicas: 3
    nginx_secret_password: "{{ vault_nginx_password }}"
  
  roles:
    - nginx_podman
```

## Usage

```bash
# Run the playbook
ansible-playbook -i inventory playbook.yml

# Access nginx
curl http://localhost:8080

# Check health
curl http://localhost:8080/health

# Manually manage pods
podman kube down /tmp/nginx-kube-manifest.yaml
podman kube play /tmp/nginx-kube-manifest.yaml
```

## Tags

- `nginx` - All nginx tasks
- `install` - Package installation
- `config` - Configuration tasks
- `deploy` - Deployment tasks

## License

MIT

## Author

Your Name
