<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# AGENTS.md

Rules for working on the console (usecode.dev).

# After a change
* When your task changed anything in `lib/console` and it is done, deploy it. Don't leave it for the user.
* First check it builds and passes, from `lib/console`: `make` (the build), and `make test` with `make test-db` up.
* `make deploy` ships what is on origin/master, not the working tree, so commit and `git push` the change first.
* Then deploy from `lib/console`, with kubectl on the cluster:

  ```sh
  make
  make deploy
  ```

* `make deploy` ends with `https://<host> is up on <sha>`; say which sha went out. If it fails, read `kubectl -n console logs deploy/console` and fix it, rather than leaving the site on a broken build.
