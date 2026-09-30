// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc daemon`: see [`uc::groups::DAEMON`].

fn main() -> std::process::ExitCode {
    uc::group::main(&uc::groups::DAEMON)
}
