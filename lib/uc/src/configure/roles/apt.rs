// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `apt`: proxy settings, sources and keys, the reachability probe for
//! optional repositories, and the package groups of each profile.

use crate::configure::engine::{Outcome, Plan, Task};
use crate::configure::modules::inflate::inflate;
use crate::configure::modules::{apt, copy, file, system};
use crate::configure::vars::{Profile, Vars};
use anyhow::bail;
use std::path::Path;
use std::time::Duration;

pub const UPDATE: &str = "apt/update";
pub const PACKAGES: &str = "apt/packages";
pub const BUILD_DEPS: &str = "apt/build-deps";
pub const PROXY: &str = "apt/proxy";
pub const SOURCES: &str = "apt/sources";
pub const PROBE: &str = "apt/probe";

/// Package groups installed for each installation profile. A group also
/// pulls in its distribution specific variant, e.g. desktop adds
/// debian_desktop.
fn profile_groups(profile: Profile) -> &'static [&'static str] {
    match profile {
        Profile::Workstation => &["build", "share", "hardware", "desktop", "virt"],
        Profile::Wsl => &["build", "share", "virt", "wsl"],
        Profile::Vm | Profile::DevContainer => &["build", "share"],
    }
}

fn group(name: &str) -> &'static [&'static str] {
    match name {
        // What building the usecode checkout needs: compilers, and the
        // headers of Python, vim and alacritty.
        "build" => &[
            "g++",                // GNU C++ compiler, for alacritty's native dependencies
            "libfreetype-dev",    // FreeType development files, for alacritty's font rendering
            "libfontconfig1-dev", // Fontconfig development files, for alacritty's font lookup
            "libxcb-xfixes0-dev", // XCB XFixes development files, for alacritty's clipboard
            "libxkbcommon-dev",   // XKB keymap development files, for alacritty's keyboard input
            "python3",            // Python interpreter, used by alacritty's build scripts
            "ncurses-bin",        // tic, which compiles alacritty's terminfo
            "desktop-file-utils", // update-desktop-database, run by alacritty's install
            "build-essential", // Meta-package for compiling C/C++ programs (gcc, g++, libc-dev, make)
            "make",            // Build automation tool for compiling programs from Makefiles
            "cmake",           // Cross-platform build system generator for modern C/C++ projects
            "gcc",             // GNU C Compiler for compiling C programs
            "libssl-dev", // OpenSSL development libraries for building programs with SSL/TLS support
            "libffi-dev", // Foreign Function Interface library for calling C code from high-level languages
            "libncurses5-dev", // Terminal UI library development files for building text-based interfaces
            "zlib1g-dev", // Compression library development files for building programs with compression
            "libreadline-dev", // Line-editing library for interactive command-line input with history
            "libbz2-dev", // Bzip2 compression library development files for building compression tools
            "libsqlite3-dev", // SQLite database library development files for embedded database applications
            "tk-dev", // Tk GUI toolkit development files for building graphical applications
            "liblzma-dev", // XZ compression library development files for high-ratio compression
            "libclang-dev", // Clang compiler libraries for building tools that parse C/C++ code
            "ncurses-dev", // Terminal handling library development files for text UI applications
            "libx11-dev", // X Window System client library for building X11 GUI applications
            "libxtst-dev", // X11 testing extension library for simulating keyboard/mouse input
            "libxt-dev", // X11 Toolkit Intrinsics library for building X11 widget-based GUIs
            "libsm-dev", // X11 Session Management library for managing application sessions
            "libxpm-dev", // X11 pixmap library for handling XPM image format in X applications
            "jq",     // Command-line JSON processor for parsing and manipulating JSON data
            "ca-certificates", // Common CA certificates for validating SSL/TLS connections
            "curl",   // Command-line HTTP client for downloading files and testing APIs
            "scdoc",  // Man page generator used to build the alacritty man pages
            "git",    // Version control, for the usecode checkout and its submodules
            "pkg-config", // Finds the compile and link flags of installed libraries
            "xz-utils", // XZ compression tools for .xz archives
            "libncurses-dev", // Wide-character ncurses development files, for building Python
            "libgdbm-dev", // GNU dbm development files, for Python's dbm.gnu
            "libgdbm-compat-dev", // GNU dbm's ndbm compatibility, for Python's dbm.ndbm
            "uuid-dev", // UUID library development files, for Python's uuid module
            "libexpat1-dev", // Expat XML parser development files, for Python's pyexpat
        ],
        "share" => &[
            "zlib1g",              // Compression library runtime for gzip/deflate operations
            "rsync", // Fast incremental file transfer tool for syncing files and backups
            "libedit-dev", // Alternative line-editing library with BSD license compatibility
            "gettext", // Internationalization and localization tools for multi-language support
            "libpcre2-dev", // Perl-compatible regex library for pattern matching in applications
            "gnupg2", // GNU Privacy Guard for encryption, signing, and key management
            "unzip", // Archive extraction utility for decompressing ZIP files
            "apt-transport-https", // APT HTTPS support for securely downloading packages over TLS
            "lsb-release", // Linux Standard Base version reporting tool for distribution detection
            "tree",  // Directory structure visualization tool for recursive file listings
            "pinentry-tty", // PIN entry dialog for GPG in terminal environments
            "zip",   // Archive compression utility for creating ZIP files
            "flex",  // Lexical analyzer generator for building compilers and interpreters
            "bison", // Parser generator for building compilers and interpreters
            "net-tools", // Legacy networking utilities (ifconfig, netstat, route) for network debugging
            "htop", // Interactive process viewer for monitoring system resources with mouse support
            "dotnet-sdk-10.0", // .NET SDK for building and running .NET applications
            "exuberant-ctags", // Code indexing tool for navigating source code in editors
            "podman", // Daemonless container engine as Docker alternative for running containers
            "libisofs-dev", // Library for creating ISO 9660 filesystem images
            "libisoburn-dev", // Library for burning ISO 9660 filesystem images to optical media
            "libburn-dev", // Library for writing data to optical media (CD/DVD/Blu-ray)
            "clangd", // Language server for C/C++ based on Clang for IDE integration
            "reprepro", // Tool for managing local APT package repositories
            "clang-format", // Code formatting tool for C/C++ based on Clang
            "ntfs-3g", // NTFS filesystem driver for read/write access to NTFS partitions
            "uuid-runtime", // Utilities for generating and managing UUIDs
            "dh-virtualenv", // Tool for creating Python virtualenvs as Debian packages
            "mplayer", // Command-line media player for playing audio and video files in terminal
            "ugrep", // Ultra-fast grep alternative with TUI and PCRE support for searching text in files
            "libnss3-tools", // Tools for managing NSS databases and certificates for secure communication
            "skopeo", // Tool for inspecting and copying container images between registries and local storage
            "tmux",   // Terminal multiplexer for managing multiple terminal sessions in one window
            "vim-nox", // Vim with Python, Lua and Ruby support, the default editor
            "ffmpeg", // Command-line multimedia framework for converting and streaming audio/video files
            "wl-clipboard", // Command-line copy/paste utilities (wl-copy, wl-paste) for clipboard integration
            "xclip", // Command-line X11 clipboard utility often used by terminal/editor integrations
            "libvirt-dev", // Development files for libvirt virtualization API for managing virtual machines
            "libglib2.0-bin", // GLib utilities for working with GObject-based libraries and applications
            "libyaml-dev",    // YAML parser library development files
            "hwdata",         // PCI and USB ID databases used by lspci and lsusb
            "usbutils",       // lsusb and friends for inspecting USB devices
        ],
        // Local KVM/QEMU virtual machines run through libvirt (`uc vm`)
        "virt" => &[
            "qemu-system-x86",       // x86/x86_64 system emulator for running virtual machines
            "qemu-utils",            // qemu-img and friends for creating and converting disk images
            "libvirt-daemon-system", // libvirtd with its systemd units, default network and pools
            "virtinst",              // virt-install for creating libvirt virtual machines
        ],
        // WSL2 only: audio goes through WSLg's PulseAudio server
        "wsl" => &[
            "sox",              // Audio recorder and converter, used by Claude Code's voice mode
            "libsox-fmt-pulse", // SoX PulseAudio backend, for recording through WSLg
        ],
        // Packages requiring physical hardware (YubiKey, smartcard) — skipped on WSL2
        "hardware" => &[
            "libpam-yubico",   // PAM module for YubiKey OTP authentication at login
            "libpcsclite-dev", // PC/SC smart card library development files for card reader applications
            "swig",            // Interface generator for connecting C/C++ with scripting languages
            "pcscd",           // PC/SC smart card daemon for managing smart card readers
            "ykcs11", // YubiKey PKCS#11 module for using YubiKey with cryptographic applications
            "scdaemon", // Smart card daemon for GPG to communicate with hardware tokens
            "libpam-u2f", // PAM module for FIDO U2F/FIDO2 hardware authentication keys
            "yubikey-manager", // ykman, for configuring YubiKeys
        ],
        // Packages requiring a desktop/GPU — skipped on WSL2
        "desktop" => &[
            "fontconfig",       // Font cache tools (fc-cache) used when installing the Nerd Fonts
            "brave-browser",    // Privacy-focused web browser with built-in ad blocking
            "yubioath-desktop", // YubiKey OATH desktop application for managing TOTP/HOTP codes
            "heif-gdk-pixbuf",  // HEIF/HEIC image format support for viewing modern image formats
            "dmidecode",        // DMI/SMBIOS decoder for reading hardware information from BIOS
            "mdns-scan",        // Multicast DNS scanner for discovering services on local network
            "gnumeric",         // Spreadsheet application for data analysis and visualization
            "mullvad-vpn", // Mullvad VPN desktop client installed from the Mullvad Apt repository
            "amberol",     // Small, focused GNOME music player for local audio files
        ],
        "debian_desktop" => &[
            "qemu-system", // Generic QEMU system emulator meta-package for all architectures
        ],
        _ => &[],
    }
}

/// A single unreachable source makes apt fail the whole cache refresh, and
/// some networks (e.g. filtering corporate proxies) block these hosts
/// outright. They are probed first and dropped when they cannot be fetched;
/// they come back automatically on a network that allows them.
/// Optional repositories: their files, dropped together when the URL does
/// not answer, so no key is left behind without its source.
const OPTIONAL_REPOS: &[(&[&str], &str)] = &[(
    &[
        "/etc/apt/sources.list.d/mullvad.sources",
        "/etc/apt/keyrings/mullvad-keyring.gpg",
    ],
    "https://repository.mullvad.net/deb/stable/dists/stable/InRelease",
)];

/// Files written by earlier versions that the templates replace. Keys in
/// `trusted.gpg.d` are trusted for every repository, so each key now lives
/// in `/etc/apt/keyrings` and only signs the source naming it; one-line
/// `.list` files became deb822 `.sources`. On Debian, extrepo hands over
/// docker, dotnet and mullvad itself, so only files no source uses go here.
fn stale(distro: &str) -> &'static [&'static str] {
    match distro {
        "Ubuntu" => &[
            "/etc/apt/sources.list.d/azure.list",
            "/etc/apt/sources.list.d/docker.list",
            "/etc/apt/sources.list.d/ms.list",
            "/etc/apt/sources.list.d/mullvad.list",
            "/etc/apt/sources.list.d/dotnetdev.list",
            "/etc/apt/sources.list.d/packages-microsoft-prod.list",
            "/etc/apt/sources.list.d/microsoft-ubuntu-resolute-prod.list",
            "/etc/apt/trusted.gpg.d/evgnomon.asc",
            "/etc/apt/trusted.gpg.d/microsoft.gpg",
            "/etc/apt/trusted.gpg.d/mullvad-keyring.gpg",
        ],
        "Debian" => &[
            "/etc/apt/trusted.gpg.d/evgnomon.asc",
            "/etc/apt/trusted.gpg.d/mysql.gpg",
        ],
        _ => &[],
    }
}

fn packages(names: &[&str]) -> Vec<String> {
    names.iter().map(|s| s.to_string()).collect()
}

pub fn tasks(plan: &mut Plan, v: &Vars) {
    let distro = v.facts.distribution.clone();

    // apt does not read http_proxy/https_proxy from the environment
    // reliably, so mirror the caller's proxy into apt's own config. Removed
    // again when the machine has no proxy, so a stale file can never break
    // plain networks.
    plan.add(
        Task::new(PROXY, "Configure the apt proxy")
            .tags(&["always"])
            .sudo()
            .run(|ctx| async move {
                let dest = Path::new("/etc/apt/apt.conf.d/99proxy");
                let v = ctx.vars();
                match v.http_proxy() {
                    Some(http) => {
                        let https = v.env_proxy("https_proxy").unwrap_or(http);
                        let text = format!(
                            "Acquire::http::Proxy \"{http}\";\nAcquire::https::Proxy \"{https}\";\n"
                        );
                        copy::content(&ctx, &text, dest, Some(0o644), true).await
                    }
                    None => file::absent(&ctx, dest, true).await,
                }
            }),
    );

    let templates = v.role("apt").join("templates").join(&distro).join("etc");
    plan.add(
        Task::new(SOURCES, "Inflate apt sources and keys into /etc")
            .tags(&["apt"])
            .sudo()
            .when(templates.is_dir(), format!("no apt templates for {distro}"))
            .run(move |ctx| async move {
                let mut outcome = inflate(&ctx, &templates, Path::new("/etc"), |_| true).await?;
                // Drop what the inflated files replace in the same step, so
                // apt never sees an old and a new file for one repository
                // with conflicting Signed-By values.
                for path in stale(&ctx.vars().facts.distribution) {
                    outcome = outcome.and(file::absent(&ctx, Path::new(path), true).await?);
                }
                Ok(outcome)
            }),
    );

    plan.add(
        Task::new(PROBE, "Drop unreachable optional apt repositories")
            .tags(&["always"])
            .sudo()
            .after([SOURCES])
            .run(|ctx| async move {
                let mut outcome = Outcome::Ok;
                for (files, url) in OPTIONAL_REPOS {
                    // Like Ansible's `failed_when: false`, a probe that cannot
                    // run counts as unreachable instead of failing the task.
                    let answer = match system::http_status(&ctx, url, 10).await {
                        Ok(200) => continue,
                        Ok(status) => format!("answered {status}"),
                        Err(err) => format!("could not be probed: {err:#}"),
                    };
                    for path in *files {
                        let dropped = file::absent(&ctx, Path::new(path), true).await?;
                        if dropped == Outcome::Changed {
                            ctx.note(&format!("dropped {path}: {url} {answer}"));
                        }
                        outcome = outcome.and(dropped);
                    }
                }
                Ok(outcome)
            }),
    );

    plan.add(
        Task::new(UPDATE, "Refresh the apt cache")
            .tags(&["apt"])
            .sudo()
            .after([PROXY, SOURCES, PROBE, "extrepo/packages"])
            .run(|ctx| async move { apt::update(&ctx, None).await }),
    );

    let groups = profile_groups(v.profile);
    let distro_lower = distro.to_lowercase();
    plan.add(
        Task::new(
            PACKAGES,
            format!("Install the {} package groups", groups.join(", ")),
        )
        .tags(&["apt"])
        .sudo()
        .after([UPDATE])
        .run(move |ctx| async move {
            // Like Ansible's loop, a failing group does not stop the others.
            let mut outcome = Outcome::Ok;
            let mut failed = Vec::new();
            for name in groups {
                let mut names = packages(group(name));
                names.extend(packages(group(&format!("{distro_lower}_{name}"))));
                match apt::install(&ctx, &names, None).await {
                    Ok(step) => outcome = outcome.and(step),
                    Err(err) => failed.push(format!("{name}: {err:#}")),
                }
            }
            if !failed.is_empty() {
                bail!("{}", failed.join("\n"));
            }
            Ok(outcome)
        }),
    );

    // Also part of `apt/packages`; on its own for what syncs only usecode.
    plan.add(
        Task::new(BUILD_DEPS, "Install the build package group")
            .tags(&["apt"])
            .sudo()
            .after([UPDATE])
            .run(|ctx| async move { apt::install(&ctx, &packages(group("build")), None).await }),
    );

    // libvirtd and /dev/kvm only let these groups in, so `uc vm` works
    // without sudo from the next login on.
    let user = v.user.clone();
    plan.add(
        Task::new(
            "apt/virt-groups",
            "Add the user to the kvm and libvirt groups",
        )
        .tags(&["apt"])
        .sudo()
        .after([PACKAGES])
        .when(
            groups.contains(&"virt"),
            "no virt package group in this profile",
        )
        .run(move |ctx| async move { system::user_groups(&ctx, &user, &["kvm", "libvirt"]).await }),
    );

    let user_packages = v.config.user_apt_packages.clone();
    let distro_lower = distro.to_lowercase();
    plan.add(
        Task::new("apt/user-packages", "Install the user's apt packages")
            .tags(&["apt"])
            .sudo()
            .after([UPDATE])
            .when(
                user_packages.is_some(),
                "no user_apt_packages in the user config",
            )
            .run(move |ctx| async move {
                let lists = user_packages.unwrap_or_default();
                let names: Vec<String> = ["share", distro_lower.as_str()]
                    .iter()
                    .filter_map(|k| lists.get(*k).cloned().flatten())
                    .flatten()
                    .collect();
                apt::install(&ctx, &names, Some(Duration::from_secs(3600))).await
            }),
    );

    plan.add(
        Task::new("apt/upgrade", "Upgrade all apt packages")
            .tags(&["never", "upgrade", "apt"])
            .sudo()
            .after([PACKAGES, "apt/user-packages"])
            .run(|ctx| async move {
                apt::update(&ctx, Some(Duration::from_secs(3600))).await?;
                apt::full_upgrade(&ctx).await
            }),
    );

    plan.add(
        Task::new("apt/autoremove", "Remove unused apt packages")
            .tags(&["never", "autoremove", "upgrade"])
            .sudo()
            .after(["apt/upgrade"])
            .run(|ctx| async move { apt::autoremove(&ctx).await }),
    );
}
