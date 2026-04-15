#!/usr/bin/env bash
set -euo pipefail

VERSION="${1:?usage: publish-galaxy.sh <version>}"
ARTIFACT="${2:?usage: publish-galaxy.sh <version> <artifact>}"
NAMESPACE="evgnomon"
NAME="catamaran"

url="https://galaxy.ansible.com/api/v3/plugin/ansible/content/published/collections/index/${NAMESPACE}/${NAME}/versions/${VERSION}/"
status=$(curl -sS -o /dev/null -w '%{http_code}' "$url")
if [ "$status" = "200" ]; then
    echo "galaxy: ${NAMESPACE}.${NAME} ${VERSION} already published, skipping"
    exit 0
fi

ansible-galaxy collection publish "$ARTIFACT" --token "$(getsecret | jq -r '.ansible_galaxy.token')"
