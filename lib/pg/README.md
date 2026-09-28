<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# uc db pg

PostgreSQL CLI: start a local instance with podman, manage databases,
tables, columns, indexes and rows, dump DDL, and run SQL with JSON output.

```sh
make build        # ../../target/release/uc-db-pg
make link         # symlink into ~/.local/bin/uc-db-pg
make install      # install into $(PREFIX)/bin (default /usr/local)
```

## Configuration

Settings come from the nearest `.pg.json`, searched from the current directory
upwards. Missing keys use these defaults:

```json
{
  "container": "pg1",
  "network": "pgnet",
  "image": "docker.io/library/postgres:17",
  "password": "t6drtfyig7",
  "user": "admin",
  "port": 5432,
  "host": "localhost",
  "database": "postgres",
  "data_dir": "~/.local/share/pg/data"
}
```

`PGHOST`, `PGPORT`, `PGUSER`, `PGPASSWORD`, `PGDATABASE` and
`PGCONNECT_TIMEOUT` (seconds, default 10) override the connection settings.

## Commands

```sh
uc db pg kickstart [--force] [--no-pull]     # run postgres in podman
uc db pg db list | add NAME | drop NAME [--force]
uc db pg db save NAME [-o FILE]              # pg_dump | gzip
uc db pg db restore FILE [-n NAME] [--no-create] # gunzip | psql
uc db pg schema [DBNAME]
uc db pg tab list | add | drop | rename | copy [--no-data]
uc db pg col list | add TABLE COL string|number|bool|jsonb [--nullable --long --double --float --unsigned --fixed N --default EXPR] | drop | rename
uc db pg idx list | add TABLE COLS... [--unique] [--gin|--no-gin] [-n NAME] | drop NAME
uc db pg row list TABLE [-l N] | rm TABLE (ID | --all)
uc db pg dump [-d DB] [-s SCHEMA]            # DDL only
uc db pg query [FILE|DIR] [-f json|table]    # SQL from a file, a dir of *.sql, or stdin
echo 'select 1' | uc db pg                   # stdin runs as a query
```

Table-level commands accept `-d DATABASE` and `-s SCHEMA` (default `public`).
Status messages go to stderr; query results go to stdout as JSON.

A single statement is run as a prepared statement, so its results keep their
types in JSON. When the SQL has several statements, only the last statement's
result is printed, and its values come back as strings.
