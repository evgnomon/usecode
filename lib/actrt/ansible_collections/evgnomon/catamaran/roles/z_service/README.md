# z\_service

Run a systemd service with a specific user and group.
The service is made out of the build outputs of the same repo.

## Example Playbook

```yaml
- name: Deploy
  hosts: shards
  gather_facts: false
  collections:
    - evgnomon.catamaran
  roles:
    - role: z_service
      vars:
        z_service_name: "zcore"
```
