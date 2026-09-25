#!/usr/bin/env bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# authors.sh — add contributors from git history to AUTHORS.
#
# Existing entries are kept as written, so contributors can change how they
# are credited by editing AUTHORS. Only authors whose email is not listed yet
# are added. Bots and automated agents are skipped.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

HEADER="# Contributors to usecode, generated from git history by scripts/authors.sh.
# To be credited under a different name or email, edit your entry."

touch AUTHORS
tmp=$(mktemp)
{
	grep -v '^#' AUTHORS | grep -v '^$' || true
	git log --use-mailmap --format='%aN <%aE>' |
		grep -vE '\[bot\]|^Copilot <' |
		sort -u |
		while read -r entry; do
			grep -qiF "<${entry##*<}" AUTHORS || echo "$entry"
		done
} | sort -fu >"$tmp"
{
	echo "$HEADER"
	echo
	cat "$tmp"
} >AUTHORS
rm -f "$tmp"
