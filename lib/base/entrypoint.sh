#!/bin/bash
# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)
#
# Starts a workspace: makes the user `uc workspace up` names in
# USECODE_USER and USECODE_UID (you, as on your laptop) with sudo, fills its
# home on a new volume, makes the host keys once, puts the keys mounted at
# /etc/usecode/authorized_keys where sshd reads them, then runs sshd in the
# foreground.
set -euo pipefail

user=${USECODE_USER:-dev}
uid=${USECODE_UID:-1000}
home=/home/$user
state=/home/.usecode/ssh

# The container is new on each start, so the user is made each time; only
# /home stays. Another user with the uid already? Then take the next free.
if ! id "$user" >/dev/null 2>&1; then
  uid_flag=(--uid "$uid")
  getent passwd "$uid" >/dev/null && uid_flag=()
  useradd "${uid_flag[@]}" --user-group --shell /bin/bash --no-create-home --home-dir "$home" "$user"
fi
# '*' instead of the locked '!': no password, and sshd still takes keys.
usermod -p '*' "$user"
echo "$user ALL=(ALL) NOPASSWD:ALL" >/etc/sudoers.d/usecode
chmod 0440 /etc/sudoers.d/usecode

if [[ ! -d $home ]]; then
  cp -a /etc/skel "$home"
fi
# A home made for another uid, by an older image or another user, is yours.
[[ $(stat -c %u "$home") == "$(id -u "$user")" ]] || chown -R "$user:$user" "$home"
chmod 0750 "$home"

# A new volume's root is often 0777, and sshd's StrictModes refuses keys
# under a directory others can write to.
chown root:root /home
chmod 0755 /home

install -d -m 0755 "$state" "$state/authorized_keys"
[[ -f $state/ssh_host_ed25519_key ]] || ssh-keygen -q -t ed25519 -N '' -f "$state/ssh_host_ed25519_key"
[[ -f $state/ssh_host_rsa_key ]] || ssh-keygen -q -t rsa -b 4096 -N '' -f "$state/ssh_host_rsa_key"

# The ConfigMap's files are symlinks sshd won't take; a root owned copy it
# will. Only this user's keys, and only this user gets in.
rm -f "$state/authorized_keys/"*
install -m 0644 /etc/usecode/authorized_keys "$state/authorized_keys/$user"
echo "AllowUsers $user" >/etc/ssh/sshd_config.d/usecode-user.conf

mkdir -p /run/sshd
exec /usr/sbin/sshd -D -e
