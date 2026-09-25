<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# z\_sign

Sign certificates

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
    - role: z_sign
      vars:
        token: mem
```
