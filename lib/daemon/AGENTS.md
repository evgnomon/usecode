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

`uc daemon install` builds `usecoded`, copies it to the host and runs
`usecoded setup`, which installs the binary and the unit - nothing else.
No keys, no vault, no packages, no feature setup.

## Setting up is the daemon's job

Each module sets its part of the host up itself once its preconditions
are there, and retries until they are. A missing precondition, or a
failure, is a note in the journal - never a failed or stopped service.
The control node only delivers files (`usecoded join`, one bundle
section per module). There is no Ansible role or playbook for
installing - don't add one back.
