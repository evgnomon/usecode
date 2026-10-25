// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc secret cert`: see [`uc::groups::CERT`].

fn main() -> std::process::ExitCode {
    uc::group::main(&uc::groups::CERT)
}
