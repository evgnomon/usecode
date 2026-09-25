<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# uc

The top level `usecode` command. `uc` itself holds no subcommand logic: like
`git`, it looks up `uc-<name>` and hands the process over to it, so a new
subcommand is added by dropping a `uc-<name>` executable next to `uc` or
anywhere on `PATH`.

```sh
uc help                # list the uc-* commands found on PATH
uc encrypt secrets.txt # runs uc-encrypt
```

This crate ships the dispatcher plus the two file encryption subcommands, which
replace the former `boom` shell script.

## Encrypting files

```sh
uc encrypt secrets.txt        # -> secrets.txt.asc, secrets.txt removed
uc decrypt secrets.txt.asc    # -> secrets.txt, the .asc kept
uc decrypt -c secrets.txt.asc # print the plaintext instead of writing it
```

Both commands prompt for the password on the terminal and never echo it.

* `-f` overwrites an existing output file; without it an existing file is an
  error.
* `uc encrypt` decrypts its own output and compares it against the input before
  it removes the original, so a failed encryption never costs the plaintext.
* `uc decrypt` accepts only `.asc` files and leaves the encrypted copy in place.
* Output files are written through a temporary sibling and renamed, with mode
  `0600`, so an interrupted run cannot leave a half-written file behind.

### File format

The `.asc` file is exactly what `openssl enc -aes-256-cbc -pbkdf2 -salt -a`
produces — an 8-byte salt behind a `Salted__` magic, AES-256-CBC with PKCS#7
padding, base64 wrapped at 64 columns — so `openssl` remains a usable fallback:

```sh
openssl enc -d -aes-256-cbc -pbkdf2 -a -in secrets.txt.asc
```

`uc decrypt` also opens files written with the pre-OpenSSL-3.0 MD5 key
derivation, and says so when it does; re-encrypting upgrades them.

## Build

```sh
make build    # cargo build --profile release --target x86_64-unknown-linux-musl
make check    # fmt --check, clippy -D warnings, tests
make install  # install uc, uc-encrypt and uc-decrypt to /usr/local/bin
```

`make install` honours `DESTDIR` (with a trailing slash) and `PREFIX`, e.g.
`make install PREFIX=$HOME/.local`.

The default target is `x86_64-unknown-linux-musl`, added with `rustup` on first
use. The crate has no C dependencies, so the result is a static-pie executable
with no shared libraries at all — it runs on any Linux of the same
architecture, including a `scratch` container, and needs no `openssl` on the
host. `TARGET=` picks another triple, e.g. a dynamically linked glibc build:

```sh
make build TARGET=x86_64-unknown-linux-gnu
```
