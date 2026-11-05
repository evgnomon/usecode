<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# Agent guide

## One daemon, growing by modules

Every host runs one service (`usecode.service`) running one binary
(`usecoded run`). Features are modules of that daemon in `src/daemon/`
(implement `Module`, list it in `modules()`) - the mesh is one of them.
Don't add a new service, binary or unit for a new feature, and don't
put daemon features into `uc-net-mesh`, which is only the mesh's CLI.

## Installing only puts the daemon on the host

`uc daemon install` gets `usecoded` (the released `usecoded-ARCH`, or a
build when run inside a checkout), copies it to the host and runs
`usecoded setup`, which installs the binary and the unit - nothing else.
No keys, no vault, no packages, no feature setup.

`usecoded` is built and packaged on its own (`make -C lib/daemon
usecoded`, `dist`), never as part of the control-node build, and uc
must never need the checkout to install it.

## C dependencies are allowed here

Unlike `uc`, `usecoded` may depend on C code: it only runs on
Debian-family hosts, where the OS can provide libraries too. Prefer a
production-grade crate with C (e.g. `ring` for TLS crypto) over an
experimental pure-Rust one. Still prefer a static binary: vendor and
link the C code statically (as `ring` does) when its license allows
static linking into usecoded. Link a library from the OS dynamically
(and add it to the .deb's `Depends`) only when it can't be linked
statically or its license doesn't allow it.

## Setting up is the daemon's job

Each module sets its part of the host up itself once its preconditions
are there, and retries until they are. A missing precondition, or a
failure, is a note in the journal - never a failed or stopped service.
The control node only delivers files (`usecoded join`, one bundle
section per module). There is no Ansible role or playbook for
installing - don't add one back.
