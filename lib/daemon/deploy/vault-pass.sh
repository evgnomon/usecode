#!/bin/sh
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# Prints the vault password for group_vars/usecode/secrets.yml.
#
# ansible.cfg points vault_password_file at this; because the file is
# executable, ansible runs it and reads the password from stdout rather
# than reading the file itself. `uc net mesh add` and `apply` resolve the
# password the same way as ansible-vault run from the repo root, with
# nothing passed on their command line.
#
# Until a host has the mesh turned on there is no secrets.yml, so there
# is nothing to decrypt and the real password isn't needed: a
# placeholder keeps the vault tools working on a machine without the
# usecode secret (uc daemon install never opens the vault;
# uc net mesh add and apply do). `uc net mesh add` sets USECODE_VAULT_WRITE=1, since
# it is about to encrypt with whatever comes out of here, and that has
# to be the real password.
#
# Only the password may reach stdout - anything else printed here is
# taken as part of it and the decrypt fails with a wrong-password error.
set -eu
secrets="${XDG_CONFIG_HOME:-$HOME/.config}/usecode/inventory/group_vars/usecode/secrets.yml"
if [ ! -e "$secrets" ] && [ -z "${USECODE_VAULT_WRITE:-}" ]; then
	echo "no-secrets-yet"
	exit 0
fi
getsecret usecode | jq -r '.vault'
