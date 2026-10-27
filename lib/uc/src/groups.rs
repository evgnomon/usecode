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
    &IMAGE, &CERT, &DEB, &DB, &NET, &DAEMON, &VM, &NEW, &AGENT, &TERMINAL, &MAIL,
];

pub static IMAGE: Group = Group {
    path: "uc image",
    summary: "move, build and run container images",
    commands: &[
        exec(
            "push",
            "push images through the bastion to the registry",
            &["uc-image-push"],
        ),
        exec(
            "pull",
            "pull images through the bastion from the registry",
            &["uc-image-pull"],
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

pub static CERT: Group = Group {
    path: "uc secret cert",
    summary: "manage X.509 certificates and trust",
    commands: &[
        exec(
            "trust",
            "trust the CA serving a TLS endpoint",
            &["uc-secret-cert-trust"],
        ),
        group("p12", &CERT_P12),
    ],
    fallback: Some(Fallback {
        argv: &["uc-secret-cert-gen"],
        summary: "certgen's local CA, server and client certificates \
                  (init, server, client, list, show, verify, delete, nginx-config)",
        bare: false,
    }),
};

pub static CERT_P12: Group = Group {
    path: "uc secret cert p12",
    summary: "bundle zygote keys and certificates as PKCS#12",
    commands: &[exec(
        "fetch",
        "fetch the zygote CA and user certificates and bundle them",
        &["uc-secret-cert-p12-fetch"],
    )],
    fallback: Some(Fallback {
        argv: &["uc-secret-cert-p12-bundle"],
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
            "read the mail on this machine, the newest first, and reply to it",
            &["uc-mail-view"],
        ),
        exec(
            "send",
            "write a new mail, with attachments, and send it",
            &["uc-mail-send"],
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
            summary: "the OpenRouter usage and credits, and the Claude subscription usage, of the agent",
            run: Run::Builtin(agent_usage),
        },
        Sub {
            name: "login",
            summary: "log in with a Claude subscription or GitHub Copilot and save the token for the chat",
            run: Run::Builtin(agent_login),
        },
    ],
    fallback: None,
};

/// `uc agent login claude|copilot [--print]`: the Claude subscription's
/// token, saved as `CLAUDE_CODE_OAUTH_TOKEN` in `~/.bashrc.d/claude.sh`, or a
/// GitHub token for Copilot, saved as `COPILOT_GITHUB_TOKEN` in
/// `~/.bashrc.d/copilot.sh`.
fn agent_login(args: &[OsString]) -> ExitCode {
    let args: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let usage = "usage: uc agent login claude|copilot [--print]";
    let copilot = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["-h" | "--help"] => {
            println!("{usage}\n");
            println!(
                "claude: logs in with your Claude subscription (Pro or Max) and saves the token"
            );
            println!("as CLAUDE_CODE_OAUTH_TOKEN in ~/.bashrc.d/claude.sh.");
            println!("copilot: logs in to GitHub with a device code and saves the token as");
            println!("COPILOT_GITHUB_TOKEN in ~/.bashrc.d/copilot.sh; the account needs Copilot.");
            println!(
                "`uc` then offers that provider's models in /model, next to the others with a key;"
            );
            println!("UC_PROVIDER=openrouter, claude or copilot picks the one it starts on.");
            println!("--print shows the export line instead of saving it.");
            return ExitCode::SUCCESS;
        }
        ["claude"] | ["claude", "--print"] => false,
        ["copilot"] | ["copilot", "--print"] => true,
        _ => {
            eprintln!("{usage}");
            return ExitCode::from(2);
        }
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let print = args.len() == 2;
    let done = if copilot {
        runtime.block_on(crate::copilot::login(print))
    } else {
        runtime.block_on(crate::claude::login(print))
    };
    match done {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc agent login: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// `uc agent usage`: the OpenRouter usage of `OPENROUTER_API_KEY`, the key
/// the coding agent itself chats with, and the Claude subscription usage of
/// `CLAUDE_CODE_OAUTH_TOKEN`. Each is shown when its credential is set.
fn agent_usage(args: &[OsString]) -> ExitCode {
    const USAGE: &str = "usage: uc agent usage [--key KEY]";
    let mut args = args.iter().map(|arg| arg.to_string_lossy().into_owned());
    let set = |name: &str| {
        std::env::var(name)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_default()
    };
    let mut key = set("OPENROUTER_API_KEY");
    let token = set(crate::claude::TOKEN_VAR);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{USAGE}\n");
                println!(
                    "Shows what the agent's own credentials have used. With OPENROUTER_API_KEY"
                );
                println!("(or --key) set: the key's spend, limits and credits. With");
                println!(
                    "CLAUDE_CODE_OAUTH_TOKEN set (uc agent login claude): how much of the Claude"
                );
                println!(
                    "subscription's 5-hour and weekly windows is used. Both are shown if both"
                );
                println!("are set.");
                return ExitCode::SUCCESS;
            }
            "--key" => {
                key = args.next().unwrap_or_default();
            }
            other => {
                eprintln!("uc agent usage: unknown argument '{other}'");
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        }
    }
    if key.trim().is_empty() && token.is_empty() {
        eprintln!(
            "set OPENROUTER_API_KEY (https://openrouter.ai/keys) or pass --key KEY, \
             or log in to Claude with `uc agent login claude` (CLAUDE_CODE_OAUTH_TOKEN)"
        );
        return ExitCode::from(2);
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("tokio runtime");
    let mut reports = Vec::new();
    let mut failed = false;
    let mut fail = |err: anyhow::Error| {
        eprintln!("uc agent usage: {err:#}");
        failed = true;
    };
    if !key.trim().is_empty() {
        match runtime.block_on(crate::openrouter::OpenRouter::new(key).usage()) {
            Ok(usage) => reports.push(usage.report()),
            Err(err) => fail(err),
        }
    }
    if !token.is_empty() {
        match runtime.block_on(crate::claude::usage_report(&token)) {
            Ok(usage) => reports.push(usage.report()),
            Err(err) => fail(err),
        }
    }
    print!("{}", reports.join("\n"));
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

pub static TERMINAL: Group = alias(
    "uc terminal",
    "browser-style windows, tabs and splits in the terminal",
    &["uc-terminal-tm"],
);
