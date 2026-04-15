#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?usage: publish-pypi.sh <version>}"
PKG="catamaran"

status=$(curl -sS -o /dev/null -w '%{http_code}' "https://pypi.org/pypi/${PKG}/${VERSION}/json")
if [ "$status" = "200" ]; then
    echo "pypi: ${PKG} ${VERSION} already published, skipping"
    exit 0
fi

UV_PUBLISH_TOKEN=$(getsecret | jq -r '.pypi.all') \
    uv publish "dist/${PKG}-${VERSION}-py3-none-any.whl" "dist/${PKG}-${VERSION}.tar.gz"
