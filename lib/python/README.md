<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# python

A full-featured CPython (ssl, sqlite3, readline, tkinter, ...) built from
source with LTO and PGO, installed under `/usr/local`. Vim links against it
for its Python 3 support.

It takes a while to build, so the root `make build` and `make install` leave
it out. Build it on its own when you need it:

```bash
make -C lib/python build-deps    # once, installs the Debian build deps
make -C lib/python build
sudo make -C lib/python install
```

The version comes from `.deb.jsonc`. `make -C lib/python dist` makes the
`cpython` Debian package in `dist/`, and `make -C lib/python help` lists the
other targets.
