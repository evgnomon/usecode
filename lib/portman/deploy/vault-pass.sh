#!/bin/sh
# Prints the vault password for group_vars/portman/secrets.yml.
#
# ansible.cfg points vault_password_file at this; because the file is
# executable, ansible runs it and reads the password from stdout rather
# than reading the file itself. `portman add` shells out to
# ansible-vault from the repo root, so it resolves the password the same
# way with nothing passed on its command line.
#
# Only the password may reach stdout - anything else printed here is
# taken as part of it and the decrypt fails with a wrong-password error.
set -eu
getsecret portman | jq -r '.vault'
