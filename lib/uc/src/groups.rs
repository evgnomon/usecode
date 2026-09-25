// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The `uc` command groups and the tools behind each command.
//!
//! Each group is run by its own `uc-<group>` executable. The tools keep their
//! own names too, so scripts calling them directly carry on working.

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
    &IMAGE, &REPO, &CERT, &DEB, &DB, &NET, &VM, &NATS, &WORK, &NEW, &CLOUD,
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
        exec(
            "ghcr",
            "build, push and delete GitHub Container Registry images",
            &["uc-ghcr"],
        ),
        group("run", &IMAGE_RUN),
    ],
    fallback: None,
};

pub static IMAGE_RUN: Group = Group {
    path: "uc image run",
    summary: "run the workflow containers on the current directory",
    commands: &[
        exec("barge", "run the barge container (was barge)", &["barge"]),
        exec(
            "yacht",
            "run the yacht container with the repository's secrets (was yacht)",
            &["yacht"],
        ),
    ],
    fallback: None,
};

pub static REPO: Group = Group {
    path: "uc repo",
    summary: "inspect and maintain git repositories",
    commands: &[
        exec(
            "fqn",
            "print the org_repo name of the current directory (was repofqn)",
            &["repofqn"],
        ),
        exec(
            "version",
            "print the latest tag on master or the branch name (was ucversion)",
            &["ucversion"],
        ),
        exec(
            "open",
            "open the origin remote in the browser (was gotorepo)",
            &["gotorepo"],
        ),
        exec(
            "status",
            "list repositories that are dirty, untracked or behind (was git_repos)",
            &["git_repos"],
        ),
        exec(
            "git-config",
            "set git identity and signing from the blueprint config (was set_git_conf)",
            &["set_git_conf"],
        ),
        exec(
            "extract",
            "extract a tool from evgnomon/flow into its own repository (was extract-tool)",
            &["extract-tool"],
        ),
        exec(
            "headers",
            "check and add the HGL license headers (was hgl)",
            &["hgl"],
        ),
        Sub {
            name: "authors",
            summary: "add contributors from git history to AUTHORS (was scripts/authors.sh)",
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
            "trust the CA serving a TLS endpoint (was trust_ca)",
            &["trust_ca"],
        ),
        group("p12", &CERT_P12),
    ],
    fallback: Some(Fallback {
        argv: &["certgen"],
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
        "fetch the zygote CA and user certificates and bundle them (was zcdump)",
        &["zcdump"],
    )],
    fallback: Some(Fallback {
        argv: &["mkp12"],
        summary: "KEYNAME bundles that function key and certificate (was mkp12)",
        bare: true,
    }),
};

pub static DEB: Group = Group {
    path: "uc deb",
    summary: "build and publish Debian packages",
    commands: &[
        exec(
            "build",
            "make Debian packages into ./dist (was mkdeb)",
            &["mkdeb"],
        ),
        exec(
            "publish",
            "publish ./dist to the apt repository (was pubdeb)",
            &["pubdeb"],
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
            "PostgreSQL instances, schemas and queries (was pg)",
            &["pg"],
        ),
        exec(
            "mongo",
            "MongoDB instances, collections and queries (was mgo)",
            &["mgo"],
        ),
        exec(
            "migrate",
            "apply SQL migrations to SQLite or PostgreSQL (was sqlize)",
            &["sqlize"],
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
            "sync resource YAMLs into PostgreSQL (was ysys sync)",
            &["ysys", "sync"],
        ),
        exec(
            "dump",
            "write the stored resources back out (was ysys dump)",
            &["ysys", "dump"],
        ),
        exec(
            "configmap",
            "query and update ConfigMaps (was confmap)",
            &["confmap"],
        ),
        exec(
            "schema",
            "create the resources table with ./pg (was k8s_ddl)",
            &["k8s_ddl"],
        ),
        exec(
            "pods",
            "run pods from the stored resources with podman (was mkpod)",
            &["mkpod"],
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
            "WireGuard mesh and port forwarding (was uc daemon)",
            &["uc-daemon"],
        ),
        exec(
            "ipsec",
            "full-mesh strongSwan IPsec VPN (was ipmesh)",
            &["ipmesh"],
        ),
        exec(
            "dig",
            "print only the A records of a DNS lookup (was diga)",
            &["diga"],
        ),
    ],
    fallback: None,
};

pub static VM: Group = alias(
    "uc vm",
    "create and run KVM/QEMU virtual machines (vm)",
    &["vm"],
);

pub static NATS: Group = alias(
    "uc nats",
    "run NATS JetStream nodes and clusters with Podman (natsup)",
    &["natsup"],
);

pub static WORK: Group = alias("uc work", "the workflow manager (workd)", &["workd"]);

pub static NEW: Group = Group {
    path: "uc new",
    summary: "scaffold roles, workflows, scripts and units",
    commands: &[
        exec(
            "role",
            "an Ansible role and its roles.yaml playbook (was mkarole)",
            &["mkarole"],
        ),
        exec(
            "workflow",
            "a Yacht GitHub workflow and default playbook (was catalyze)",
            &["catalyze"],
        ),
        exec(
            "script",
            "an executable script with a shebang (was shole)",
            &["shole"],
        ),
        exec(
            "unit",
            "a systemd unit running a command (was mkunit)",
            &["mkunit"],
        ),
    ],
    fallback: None,
};

pub static CLOUD: Group = Group {
    path: "uc cloud",
    summary: "cloud providers and deployments",
    commands: &[
        exec(
            "do",
            "doctl with the repository's DigitalOcean token (was wdoctl)",
            &["wdoctl"],
        ),
        exec(
            "hcloud",
            "hcloud with a generated config (was whcloud)",
            &["whcloud"],
        ),
        group("play", &CLOUD_PLAY),
    ],
    fallback: None,
};

pub static CLOUD_PLAY: Group = Group {
    path: "uc cloud play",
    summary: "run playbooks, per-host deployments and remote scripts",
    commands: &[
        exec("host", "per-host service deployments (was plat)", &["plat"]),
        exec(
            "ssh",
            "run a script and upload files on several servers (was annabelle)",
            &["annabelle"],
        ),
    ],
    fallback: Some(Fallback {
        argv: &["y"],
        summary: "run the repository playbook with its vault secrets, \
                  passing the arguments to ansible-playbook (was y)",
        bare: true,
    }),
};
