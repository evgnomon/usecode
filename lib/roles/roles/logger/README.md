# logger

Deploys [Vector](https://vector.dev) as a podman Quadlet unit to collect logs from
the host's systemd journal and `/var/log` files, writing NDJSON to a local
directory. A systemd timer prunes old output on a schedule.

## What it does

- Pulls `docker.io/timberio/vector:latest-alpine` (overridable).
- Drops a Quadlet container unit (`/etc/containers/systemd/vector.container`) so
  systemd manages Vector as `vector.service`.
- Bind-mounts the host's `/var/log/journal` (read-only) and `/var/log`
  (read-only, as `/hostlog` inside the container) into Vector.
- Renders `/etc/vector/vector.yaml` with two sources (`journald`, file tail of
  `/var/log/**`), one normalize transform, and one file sink.
- Installs `vector-retention.service` + `vector-retention.timer` to delete
  collected NDJSON older than N days.

## Where logs go

Collected logs are written to the host at:

```
/var/log/collected/<hostname>/<log_group>/<YYYY-MM-DD>T<HH>.ndjson
```

- `<log_group>` is the podman container name (`CONTAINER_NAME` from journald)
  when the event came from a container, and `sys` for everything else
  (host journal entries, kernel, `/var/log/*` files).
- One file per `(host, log_group)` per hour, newline-delimited JSON, no
  compression. Base path is controlled by `logger_output_dir`.

Example layout:

```
/var/log/collected/myhost/
├── sys/2026-05-15T10.ndjson
├── nginx/2026-05-15T10.ndjson
└── postgres/2026-05-15T10.ndjson
```

Each event is enriched by the `normalize` transform:

| Field        | Source                                            |
| ------------ | ------------------------------------------------- |
| `host`       | `$HOST_HOSTNAME` env (set to `%H` by the unit)    |
| `ingest_ts`  | Time Vector saw the event                         |
| `src`        | `journald` or `file`                              |
| `unit`       | systemd unit (journal events)                     |
| `pid`, `uid` | from journal metadata                             |
| `container`  | `CONTAINER_NAME` when present                     |
| `log_group`  | `.container` when present, else `"sys"` — used in the output path |
| `path`       | Original host path (file events; `/hostlog/` → `/var/log/`) |
| `parsed`     | Parsed object when `message` is valid JSON        |

## Requirements

- podman 4.4+ (Quadlet support)
- systemd
- Ansible collection: `containers.podman`

Install collections with:

```sh
ansible-galaxy collection install -r requirements.yml
```

## Role variables

All variables live in `defaults/main.yml` and can be overridden in your play,
inventory, or `--extra-vars`.

### Image / runtime

| Variable              | Default                                       | Meaning                            |
| --------------------- | --------------------------------------------- | ---------------------------------- |
| `logger_image`        | `docker.io/timberio/vector:latest-alpine`     | Vector container image             |
| `logger_memory_limit` | `512m`                                        | Passed to `podman --memory`        |
| `logger_cpu_limit`    | `"1.0"`                                       | Passed to `podman --cpus`          |

### Paths

| Variable              | Default                       | Meaning                                  |
| --------------------- | ----------------------------- | ---------------------------------------- |
| `logger_config_dir`   | `/etc/vector`                 | Where `vector.yaml` is installed         |
| `logger_quadlet_dir`  | `/etc/containers/systemd`     | Where the Quadlet unit is installed      |
| `logger_output_dir`   | `/var/log/collected`          | Where NDJSON files are written           |
| `logger_systemd_dir`  | `/etc/systemd/system`         | Where the retention units are installed  |

### Sources — journal

| Variable                              | Default       | Meaning                                                  |
| ------------------------------------- | ------------- | -------------------------------------------------------- |
| `logger_journal_exclude_containers`   | `[vector]`    | `CONTAINER_NAME` values dropped from the journal source  |

### Sources — files

| Variable                          | Default                                                  | Meaning                                  |
| --------------------------------- | -------------------------------------------------------- | ---------------------------------------- |
| `logger_file_include`             | `/hostlog/**/*.log`, `/hostlog/syslog`, `auth.log`, `kern.log` | Glob patterns Vector tails (relative to the container's `/hostlog`, which is the host's `/var/log`) |
| `logger_file_exclude`             | `/hostlog/collected/**`, `/hostlog/vector/**`            | Globs to skip — keep `collected/` here or you'll loop your own output |
| `logger_file_ignore_older_secs`   | `86400`                                                  | Skip files untouched longer than this    |

### Retention

| Variable                      | Default   | Meaning                                            |
| ----------------------------- | --------- | -------------------------------------------------- |
| `logger_retention_days`       | `3`       | NDJSON files older than this are deleted           |
| `logger_retention_on_boot`    | `10min`   | Timer's `OnBootSec`                                |
| `logger_retention_interval`   | `1h`      | Timer's `OnUnitActiveSec` (how often pruning runs) |

## Usage

```yaml
- hosts: log_hosts
  become: true
  roles:
    - role: logger
      vars:
        logger_retention_days: 7
        logger_file_include:
          - /hostlog/**/*.log
          - /hostlog/nginx/access.log
```

Run the bundled playbook against localhost:

```sh
./run.sh                 # apply
./run.sh --check         # dry run
./run.sh --tags logger   # if you add more roles later
```

## Operating

```sh
systemctl status vector.service           # service state
journalctl -u vector -f                   # live logs (Vector's own stderr)
ls -lh /var/log/collected/$(hostname)/    # collected output
systemctl list-timers vector-retention.timer
```

Restart after editing `vars` or templates: the role notifies a handler that
reloads systemd and restarts `vector.service` on any config/unit change.

## Notes

- The Quadlet unit sets `LogDriver=none`, so `podman logs vector` shows nothing.
  Use `journalctl -u vector` instead.
- The role validates `vector.yaml` by running `vector validate` in a throwaway
  container before starting the service — a bad config fails the play, not the
  running service.
- `logger_file_exclude` defaults already skip the role's own output directory.
  If you change `logger_output_dir` to something outside `/var/log`, update the
  excludes (or the bind mount) so Vector doesn't tail what it's writing.
