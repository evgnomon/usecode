ssh-server
==========

Hardens the OpenSSH server: disables password/keyboard-interactive login
(key-only auth), sets `PermitRootLogin` to key-only, and moves `sshd` off
port 22.

All settings are written to a single drop-in fragment,
`00-usecode-agent-hardening.conf`, in `/etc/ssh/sshd_config.d/` — sorted first
so it's read (and therefore wins, since `sshd` uses the first value seen
for a given keyword) ahead of any other drop-in or the rest of
`sshd_config`. Nothing else in `sshd_config` is touched, other than
adding the `Include` directive itself if the target is old enough to be
missing it.

**Before applying this role, make sure you have working key-based access
as a non-root user** (or as root, if you intend to keep
`ssh_permit_root_login: prohibit-password`) — password auth is switched
off in the same run that changes the port, so there is no fallback once
it applies.

Also run the `firewall` role first with the new port
(`ssh_port`, default `2657`) already present in `firewall_allow_rules` —
otherwise you'll be locked out the moment `sshd` restarts on the new
port. Reboot after a `firewall_allow_rules` change per that role's own
README before relying on it here.

Safety checks
-------------

- Every config write is validated with `sshd -t -f %s` (syntax-checked
  before it's copied into place), and a final `sshd -t` checks the fully
  merged configuration before any handler fires. If validation fails the
  play stops and `sshd` is never restarted, so a bad change can't lock
  you out.
- `KbdInteractiveAuthentication` was only added in OpenSSH 8.7. On older
  targets (e.g. Debian 11/Ubuntu 20.04 and earlier) the role detects this
  and falls back to the legacy `ChallengeResponseAuthentication` keyword
  it replaced, controlled by `ssh_challenge_response_authentication`.

Requirements
------------

- Debian/Ubuntu target (uses `apt` for `openssh-server`).

Role Variables
--------------

- `ssh_port` — port `sshd` listens on (default `2657`).
- `ssh_permit_root_login` — default `prohibit-password` (root may still
  log in with a key, not a password). Set to `no` once you have a
  non-root user with sudo + key-based access provisioned.
- `ssh_password_authentication` — default `no`.
- `ssh_kbd_interactive_authentication` — default `no`.
- `ssh_pubkey_authentication` — default `yes`.
- `ssh_challenge_response_authentication` — default `no`; used instead of
  `ssh_kbd_interactive_authentication` on OpenSSH < 8.7 (see above).
- `ssh_sshd_config_dir` — default `/etc/ssh/sshd_config.d`.
- `ssh_hardening_config_file` — rendered fragment path, default
  `{{ ssh_sshd_config_dir }}/00-usecode-agent-hardening.conf`.
- `ssh_sshd_config_path` — default `/etc/ssh/sshd_config`.

Example Playbook
----------------

    - hosts: main
      roles:
        - role: firewall
          firewall_bastion_hosts:
            - "203.0.113.10"
          firewall_allow_rules:
            - "tcp:2657:*"
        - role: ssh-server
          ssh_port: 2657

License
-------

MIT
