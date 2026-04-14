# ensure_no_ipv6

Make sure IPv6 is not enabled on the system.

## Requirements

- Ansible 2.9 or higher
- Supported platforms:
  - EL 7
  - EL 8

## Role Variables


## Example Playbook

```yaml
- hosts: all
  roles:
    - role: ensure_no_ipv6
      become: true
```
