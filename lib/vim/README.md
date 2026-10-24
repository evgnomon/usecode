<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# vim

Vim built from source with huge features and Python 3, plus the `.vimrc`,
`.ideavimrc` and coc settings that go with it.

The root `make install` leaves vim out, so install it on its own. It links
against the CPython in `/usr/local`, so build and install
[python](../python/README.md) first:

```bash
make -C lib/python build && sudo make -C lib/python install
make -C lib/vim build
sudo make -C lib/vim install
```

To use the config, link it into your home and fetch vim-plug:

```bash
make -C lib/vim init
```

`make -C lib/vim update` pulls the latest vim source, and
`make -C lib/vim dist` makes the `uc-vim` Debian package in `dist/`.
