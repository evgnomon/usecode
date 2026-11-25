# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

# /etc/profile.d/mise.sh: the system-wide mise tools (uc and the rest of
# /etc/mise/config.toml) on PATH in login shells. /etc/environment does the
# same for ssh commands.
case ":$PATH:" in
  *:/usr/local/share/mise/shims:*) ;;
  *) PATH=/usr/local/share/mise/shims:$PATH ;;
esac
export PATH
