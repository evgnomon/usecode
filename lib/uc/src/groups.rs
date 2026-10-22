// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The `uc` command groups and the tools behind each command.
//!
//! Each group is run by its own `uc-<group>` executable. A tool is named
//! after the command that runs it, `uc db resources sync` runs
//! `uc-db-resources-yaml sync`, and can be run by that name directly too.

use crate::group::{Fallback, Group, Run, Sub};
use std::ffi::OsString;
use std::process::ExitCode;

const fn exec(name: &'static str, summary: &'static str, argv: &'static [&'static str]) -> Sub {
    Sub {
        name,
        summary,
        run: Run::Exec(argv),
    }
}

const fn group(name: &'static str, group: &'static Group) -> Sub {
    Sub {
        name,
        summary: group.summary,
        run: Run::Group(group),
    }
}

/// A group that is only a new name for one tool.
const fn alias(path: &'static str, summary: &'static str, argv: &'static [&'static str]) -> Group {
    Group {
        path,
        summary,
        commands: &[],
        fallback: Some(Fallback {
            argv,
            summary,
            bare: true,
        }),
    }
}

pub const ALL: &[&Group] = &[
    &IMAGE, &REPO, &CERT, &DEB, &DB, &NET, &DAEMON, &VM, &NEW, &AGENT, &DATA, &PICK, &SYS,
    &TERMINAL, &MAIL,
];

pub static IMAGE: Group = Group {
    path: "uc image",
    summary: "move, build and run container images",
    commands: &[
        exec(
            "push",
            "push images through the bastion to the registry",
            &["uc-push"],
        ),
        exec(
            "pull",
            "pull images through the bastion from the registry",
            &["uc-pull"],
        ),
        group("run", &IMAGE_RUN),
    ],
    fallback: None,
};

pub static IMAGE_RUN: Group = Group {
    path: "uc image run",
    summary: "run the workflow containers on the current directory",
    commands: &[exec(
        "barge",
        "run the barge container",
        &["uc-image-run-barge"],
    )],
    fallback: None,
};

pub static REPO: Group = Group {
    path: "uc repo",
    summary: "inspect and maintain git repositories",
    commands: &[
        exec(
            "fqn",
            "print the org_repo name of the current directory",
            &["uc-repo-fqn"],
        ),
        exec(
            "version",
            "print the latest tag on master or the branch name",
            &["uc-repo-version"],
        ),
        exec(
            "open",
            "open the origin remote in the browser",
            &["uc-repo-open"],
        ),
        exec(
            "status",
            "list repositories that are dirty, untracked or behind",
            &["uc-repo-status"],
        ),
        exec(
            "git-config",
            "set git identity and signing from the blueprint config",
            &["uc-repo-git-config"],
        ),
        exec(
            "extract",
            "extract a tool from evgnomon/flow into its own repository",
            &["uc-repo-extract"],
        ),
        exec(
            "headers",
            "check and add the HGL license headers",
            &["uc-repo-headers"],
        ),
        Sub {
            name: "authors",
            summary: "add contributors from git history to AUTHORS",
            run: Run::Builtin(authors),
        },
    ],
    fallback: None,
};

fn authors(args: &[OsString]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("usage: uc repo authors");
        return ExitCode::from(2);
    }
    match crate::authors::update() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc repo authors: {err:#}");
            ExitCode::FAILURE
        }
    }
}

pub static CERT: Group = Group {
    path: "uc cert",
    summary: "manage X.509 certificates and trust",
    commands: &[
        exec(
            "trust",
            "trust the CA serving a TLS endpoint",
            &["uc-cert-trust"],
        ),
        group("p12", &CERT_P12),
    ],
    fallback: Some(Fallback {
        argv: &["uc-cert-gen"],
        summary: "certgen's local CA, server and client certificates \
                  (init, server, client, list, show, verify, delete, nginx-config)",
        bare: false,
    }),
};

pub static CERT_P12: Group = Group {
    path: "uc cert p12",
    summary: "bundle zygote keys and certificates as PKCS#12",
    commands: &[exec(
        "fetch",
        "fetch the zygote CA and user certificates and bundle them",
        &["uc-cert-p12-fetch"],
    )],
    fallback: Some(Fallback {
        argv: &["uc-cert-p12-bundle"],
        summary: "KEYNAME bundles that function key and certificate",
        bare: true,
    }),
};

pub static DEB: Group = Group {
    path: "uc deb",
    summary: "build and publish Debian packages",
    commands: &[
        exec(
            "build",
            "make Debian packages into ./dist",
            &["uc-deb-build"],
        ),
        exec(
            "publish",
            "publish ./dist to the apt repository",
            &["uc-deb-publish"],
        ),
    ],
    fallback: None,
};

pub static DB: Group = Group {
    path: "uc db",
    summary: "run and manage local databases",
    commands: &[
        exec(
            "pg",
            "PostgreSQL instances, schemas and queries",
            &["uc-db-pg"],
        ),
        exec(
            "mongo",
            "MongoDB instances, collections and queries",
            &["uc-db-mongo"],
        ),
        exec(
            "migrate",
            "apply SQL migrations to SQLite or PostgreSQL",
            &["uc-db-migrate"],
        ),
        group("resources", &DB_RESOURCES),
    ],
    fallback: None,
};

pub static DB_RESOURCES: Group = Group {
    path: "uc db resources",
    summary: "Kubernetes-style resources kept in PostgreSQL",
    commands: &[
        exec(
            "sync",
            "sync resource YAMLs into PostgreSQL",
            &["uc-db-resources-yaml", "sync"],
        ),
        exec(
            "dump",
            "write the stored resources back out",
            &["uc-db-resources-yaml", "dump"],
        ),
        exec(
            "configmap",
            "query and update ConfigMaps",
            &["uc-db-resources-configmap"],
        ),
        exec(
            "schema",
            "create the resources table with uc db pg",
            &["uc-db-resources-schema"],
        ),
        exec(
            "pods",
            "run pods from the stored resources with podman",
            &["uc-db-resources-pods"],
        ),
    ],
    fallback: None,
};

pub static NET: Group = Group {
    path: "uc net",
    summary: "mesh networks, VPNs and DNS",
    commands: &[
        exec(
            "mesh",
            "WireGuard mesh and port forwarding",
            &["uc-net-mesh"],
        ),
        exec("ipsec", "full-mesh strongSwan IPsec VPN", &["uc-net-ipsec"]),
        exec(
            "firewall",
            "default-deny inbound firewall on hosts",
            &["uc-net-firewall"],
        ),
        exec(
            "dig",
            "print only the A records of a DNS lookup",
            &["uc-net-dig"],
        ),
    ],
    fallback: None,
};

pub static MAIL: Group = Group {
    path: "uc mail",
    summary: "read your mail and look after the mail server on your cluster",
    commands: &[
        exec(
            "pull",
            "fetch new mail from your accounts to this machine",
            &["uc-mail-pull"],
        ),
        exec(
            "view",
            "read the mail on this machine, the newest first",
            &["uc-mail-view"],
        ),
        exec(
            "send",
            "write a new mail, with attachments, and send it",
            &["uc-mail-send"],
        ),
        exec(
            "reply",
            "reply to a mail, to the sender or to all",
            &["uc-mail-reply"],
        ),
        exec(
            "configure",
            "DNS, reverse DNS and Stalwart settings for trusted mail",
            &["uc-mail-configure"],
        ),
        exec(
            "settings",
            "IMAP and SMTP settings for mail apps",
            &["uc-mail-settings"],
        ),
        exec(
            "domain",
            "the domains the mail server takes mail for",
            &["uc-mail-domain"],
        ),
        exec(
            "account",
            "mail accounts, their passwords and aliases",
            &["uc-mail-account"],
        ),
    ],
    fallback: None,
};

pub static DAEMON: Group = Group {
    path: "uc daemon",
    summary: "install and reload the usecode daemon on hosts",
    commands: &[
        exec(
            "install",
            "install the usecode daemon on a host with systemd",
            &["uc-daemon-ctl", "install"],
        ),
        exec(
            "reload",
            "have the daemon on hosts converge every module now",
            &["uc-daemon-ctl", "reload"],
        ),
    ],
    fallback: None,
};

pub static VM: Group = alias(
    "uc vm",
    "create and run virtual machines locally with KVM/QEMU or on Hetzner, DigitalOcean, OVHcloud and UpCloud",
    &["uc-vm-local"],
);

pub static NEW: Group = Group {
    path: "uc new",
    summary: "scaffold scripts and units",
    commands: &[
        exec(
            "script",
            "an executable script with a shebang",
            &["uc-new-script"],
        ),
        exec("unit", "a systemd unit running a command", &["uc-new-unit"]),
    ],
    fallback: None,
};

pub static AGENT: Group = Group {
    path: "uc agent",
    summary: "the usecode agent's HTTP backend and MCP server",
    commands: &[
        exec(
            "api",
            "HTTP backend: OTP auth, API keys, cloud servers, tasks and the chat UI",
            &["uc-agent-api"],
        ),
        exec(
            "mcp",
            "MCP server for operating the usecode agent",
            &["uc-agent-mcp"],
        ),
        Sub {
            name: "usage",
            summary: "the OpenRouter usage and credits of the agent's own key",
            run: Run::Builtin(agent_usage),
        },
    ],
    fallback: None,
};

/// `uc agent usage`: the OpenRouter usage of `OPENROUTER_API_KEY`, the key
/// the coding agent itself chats with.
fn agent_usage(args: &[OsString]) -> ExitCode {
    let mut args = args.iter().map(|arg| arg.to_string_lossy().into_owned());
    let mut key = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .unwrap_or_default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("usage: uc agent usage [--key KEY]");
                return ExitCode::SUCCESS;
            }
            "--key" => {
                key = args.next().unwrap_or_default();
            }
            other => {
                eprintln!("uc agent usage: unknown argument '{other}'");
                eprintln!("usage: uc agent usage [--key KEY]");
                return ExitCode::from(2);
            }
        }
    }
    if key.trim().is_empty() {
        eprintln!("set OPENROUTER_API_KEY (https://openrouter.ai/keys), or pass --key KEY");
        return ExitCode::from(2);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let result = runtime.block_on(crate::openrouter::OpenRouter::new(key).usage());
    match result {
        Ok(usage) => {
            print!("{}", usage.report());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("uc agent usage: {err:#}");
            ExitCode::FAILURE
        }
    }
}

pub static DATA: Group = Group {
    path: "uc data",
    summary: "convert and tidy CSV and JSON",
    commands: &[
        exec(
            "pdf",
            "render CSV from stdin as a PDF table",
            &["uc-data-pdf"],
        ),
        exec(
            "tidy",
            "align CSV columns so the file reads well as plain text",
            &["uc-data-tidy"],
        ),
        exec(
            "jsonc",
            "strip comments and trailing commas from JSONC",
            &["uc-data-jsonc"],
        ),
    ],
    fallback: None,
};

pub static PICK: Group = Group {
    path: "uc pick",
    summary: "fuzzy-pick files and URLs with fzf",
    commands: &[
        exec(
            "file",
            "fuzzy-pick files under a directory",
            &["uc-pick-file"],
        ),
        exec(
            "url",
            "open URLs, files, searches or picked bookmarks in Brave",
            &["uc-pick-url"],
        ),
    ],
    fallback: None,
};

pub static SYS: Group = Group {
    path: "uc sys",
    summary: "small helpers for the local machine",
    commands: &[
        exec(
            "yubikey",
            "attach a YubiKey to WSL and show its info",
            &["uc-sys-yubikey"],
        ),
        exec(
            "vi",
            "hardened vim without config, plugins, backups or modelines",
            &["uc-sys-vi"],
        ),
        exec(
            "argv",
            "print argc and each argv element with its index",
            &["uc-sys-argv"],
        ),
    ],
    fallback: None,
};

pub static TERMINAL: Group = alias(
    "uc terminal",
    "browser-style windows, tabs and splits in the terminal",
    &["uc-terminal-tm"],
);
