// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

fn main() {
    cc::Build::new()
        .file("src/iso.c")
        .include("src")
        .compile("vm_iso");

    println!("cargo:rustc-link-lib=virt");
    println!("cargo:rustc-link-lib=isoburn");
    println!("cargo:rustc-link-lib=isofs");
    println!("cargo:rustc-link-lib=burn");
    println!("cargo:rerun-if-changed=src/iso.c");
    println!("cargo:rerun-if-changed=src/iso.h");
}
