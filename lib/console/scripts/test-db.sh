#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# A throwaway Postgres in podman for `make test` and `make serve`.
# Usage: test-db.sh start <port> | stop

set -euo pipefail

name=uc-console-test-db

case "${1:?start or stop}" in
start)
    podman rm -f "$name" >/dev/null 2>&1 || true
    podman run -d --name "$name" -e POSTGRES_PASSWORD=test -e POSTGRES_DB=console \
        -p "127.0.0.1:${2:?port}:5432" docker.io/library/postgres:17-alpine >/dev/null
    for _ in $(seq 30); do
        podman exec "$name" pg_isready -U postgres -q && exit 0
        sleep 1
    done
    echo "postgres did not come up" >&2
    exit 1
    ;;
stop)
    podman rm -f "$name" >/dev/null
    ;;
esac
