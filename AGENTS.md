<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# AGENTS.md

General rules for working in the usecode project.


# General rules for working in the usecode project:
* Remove if's inside make files to keep them short.
* Long logic should be implemented in scripts, not make files.
* Prefer Rust-native solutions in the Rust code. Call HTTP APIs directly with `reqwest` (rustls) instead of running `curl`, a vendor CLI (`cf`, `hcloud`, ...) or `dig`, and use a crate before shelling out to a tool. Keep crates that build static for musl (like `uc`) free of C: use reqwest's `rustls-no-provider` with the pure-Rust `rustls-graviola` provider, not the default aws-lc or ring. Only run external programs when they are the thing being managed (`kubectl`, `helm`, `systemctl`, ...).

# Docs, README and such:
* use a friendly language in a friendly way, that means "here's something I made that might be useful for you", instead of acting like you're some big giant new startup coming to change the world.
* be a seller.

# Architecture: the usecode daemon (lib/daemon)
* Every host runs one service, `usecode.service`, running one binary: `usecoded run`.
* Features are modules of that daemon in `lib/daemon/src/daemon/` (implement `Module`, add it to `modules()`); the WireGuard mesh is the first. A new feature (k8s, log collection, ...) is a new module, not a new service or binary.
* Each module sets its part of the host up itself once its preconditions are there, and retries until they are. It logs what it's waiting for and never fails the service.
* `uc daemon install` only puts `usecoded` and the unit on a host. The control node only delivers files (`usecoded join`, one bundle section per module, e.g. `uc net mesh apply`).
* No Ansible role or playbook for installing. See `lib/daemon/AGENTS.md` for details.
