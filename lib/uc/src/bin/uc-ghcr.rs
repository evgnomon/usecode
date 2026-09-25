// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-ghcr` — build, push and delete images on the GitHub Container
//! Registry.

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use uc::ghcr::{self, Api, Build, Target};

const SUMMARY: &str = "build, push and delete images on the GitHub Container Registry";

/// Build, push and delete images on the GitHub Container Registry.
///
/// The token comes from --token or GHCR_TOKEN. Building needs read:packages
/// (write:packages to push); deleting needs delete:packages.
#[derive(Parser)]
#[command(name = "uc ghcr", bin_name = "uc ghcr", version)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Log in, build ghcr.io/<owner>/<image>:<tag> and optionally push it.
    Build {
        #[command(flatten)]
        target: Target,

        /// Tag to build; slashes become dashes, so a branch name works.
        #[arg(short, long)]
        tag: String,

        /// Dockerfile to build from.
        #[arg(short, long, default_value = "Dockerfile")]
        file: PathBuf,

        /// Build context.
        #[arg(short = 'C', long, default_value = ".")]
        context: PathBuf,

        /// Push the image after building it.
        #[arg(long)]
        push: bool,

        /// Container CLI used to log in, build and push.
        #[arg(long, env = "CONTAINER_CLI", default_value = "docker")]
        cli: String,
    },
    /// Delete the image version carrying a tag.
    Delete {
        #[command(flatten)]
        target: Target,

        /// Tag (or digest) of the version to delete; slashes become dashes.
        #[arg(short, long)]
        tag: String,

        /// GitHub REST API root.
        #[arg(long, env = "GITHUB_API_URL", default_value = "https://api.github.com")]
        api_url: String,
    },
}

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("{SUMMARY}");
        return ExitCode::SUCCESS;
    }
    let result = match Cli::parse().command {
        Cmd::Build {
            target,
            tag,
            file,
            context,
            push,
            cli,
        } => ghcr::build(
            &target,
            &Build {
                tag,
                file,
                context,
                push,
                cli,
            },
        ),
        Cmd::Delete {
            target,
            tag,
            api_url,
        } => {
            let tag = ghcr::normalize_tag(&tag);
            Api::new(&api_url, &target.token)
                .delete_tag(&target.owner, &target.image, &tag)
                .map(|id| println!("Deleted {} (version {id})", target.reference(&tag)))
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc-ghcr: {err:#}");
            ExitCode::FAILURE
        }
    }
}
