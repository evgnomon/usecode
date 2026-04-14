# z\_install

Make an executable available in the remote host.

For Go projects, uploads dist/* to the remote host and makes it executable.
Only the compatible architecture is uploaded.

## Example Playbook

```yaml
- hosts: all
  roles:
    - role: z_install
```
