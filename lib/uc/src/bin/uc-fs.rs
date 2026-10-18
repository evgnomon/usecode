// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-fs`: `uc fs`, see [`uc::fs`].

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use uc::fs::{Store, human, remote, split};
use uc::kube;

/// Buckets and objects on your S3 object storage.
///
/// It talks to the RustFS that `uc kube sync -t rustfs` runs on your
/// cluster, finding its name and keys there, or to any S3 endpoint given
/// with UC_FS_ENDPOINT, UC_FS_ACCESS_KEY and UC_FS_SECRET_KEY.
#[derive(Parser)]
#[command(name = "uc fs", bin_name = "uc fs", version, after_help = EXAMPLES)]
struct Cli {
    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT", global = true)]
    context: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// The buckets, or what is in a bucket under a prefix.
    Ls {
        /// `<bucket>:[<prefix>]` [default: list the buckets].
        path: Option<String>,
        /// Everything under the prefix, not just one level.
        #[arg(short, long)]
        recursive: bool,
    },
    /// Make a bucket.
    Mb { bucket: String },
    /// Remove an empty bucket.
    Rb { bucket: String },
    /// Copy files and directories to and from buckets, like cp.
    Cp {
        /// What to copy: `<bucket>:<key>` in a bucket, a local path
        /// otherwise.
        #[arg(required = true)]
        sources: Vec<String>,
        /// Where to: `<bucket>:` or `<bucket>:<prefix>/` keeps the names,
        /// `<bucket>:<key>` renames; a local file or directory.
        dest: String,
        /// Copy directories (and prefixes) with what is in them.
        #[arg(short, long)]
        recursive: bool,
    },
    /// Print an object.
    Cat {
        /// `<bucket>:<key>`.
        path: String,
    },
    /// Remove objects.
    Rm {
        /// `<bucket>:<key>`.
        #[arg(required = true)]
        paths: Vec<String>,
        /// Everything under a prefix.
        #[arg(short, long)]
        recursive: bool,
    },
}

const EXAMPLES: &str = "\
Examples:
  uc fs mb backups                         a bucket
  uc fs cp db.tar.gz backups:              backups:db.tar.gz
  uc fs cp a.txt b.txt backups:2026/       backups:2026/a.txt and b.txt
  uc fs cp notes.md backups:today.md       under another name
  uc fs cp -r scripts backups:             backups:scripts/...
  uc fs ls                                 the buckets
  uc fs ls backups:2026/                   one level, -r for all of it
  uc fs cp backups:db.tar.gz .             into ./db.tar.gz
  uc fs cp -r backups:scripts .            into ./scripts/
  uc fs cat backups:today.md
  uc fs rm backups:2026/a.txt              -r for all of backups:2026/
  uc fs rb backups                         once it's empty

Elsewhere than on the cluster's RustFS:
  UC_FS_ENDPOINT=https://s3.example.com UC_FS_ACCESS_KEY=... UC_FS_SECRET_KEY=... uc fs ls";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("buckets and objects on your S3 object storage");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let result = tokio::runtime::Runtime::new()
        .map_err(Into::into)
        .and_then(|rt| rt.block_on(run(cli)));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc fs: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let context = cli.context;
    let store = Store::connect(|| context.map_or_else(kube::cluster::current_context, Ok)).await?;
    match cli.command {
        Command::Ls { path: None, .. } => {
            for (name, created) in store.buckets().await? {
                println!("{created:24}  {name}/");
            }
        }
        Command::Ls {
            path: Some(path),
            recursive,
        } => {
            let (bucket, prefix) = split(&path)?;
            let listing = store.list(&bucket, &prefix, recursive).await?;
            for p in &listing.prefixes {
                println!("{:24}  {:>10}  {p}", "", "DIR");
            }
            for o in &listing.objects {
                println!("{:24}  {:>10}  {}", o.modified, human(o.size), o.key);
            }
        }
        Command::Mb { bucket } => {
            store.make_bucket(bucket.trim_matches('/')).await?;
            eprintln!("made {bucket} on {}", store.endpoint());
        }
        Command::Rb { bucket } => {
            store.remove_bucket(bucket.trim_matches('/')).await?;
            eprintln!("removed {bucket}");
        }
        Command::Cp {
            sources,
            dest,
            recursive,
        } => copy(&store, &sources, &dest, recursive).await?,
        Command::Cat { path } => {
            let (bucket, key) = object(&path)?;
            cat(&store, &bucket, &key).await?;
        }
        Command::Rm { paths, recursive } => {
            for path in &paths {
                let (bucket, key) = if recursive {
                    split(path)?
                } else {
                    object(path)?
                };
                let keys = if recursive {
                    match prefix_or_object(&store, &bucket, &key).await? {
                        Some(keys) => keys.into_iter().map(|(k, _)| k).collect(),
                        None => vec![key],
                    }
                } else {
                    vec![key]
                };
                for key in keys {
                    store.remove(&bucket, &key).await?;
                    eprintln!("removed {bucket}:{key}");
                }
            }
        }
    }
    Ok(())
}

/// A local file or an object.
enum Place {
    Local(PathBuf),
    Remote(String, String),
}

impl std::fmt::Display for Place {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Place::Local(p) => write!(f, "{}", p.display()),
            Place::Remote(b, k) => write!(f, "{b}:{k}"),
        }
    }
}

/// What one source of `cp` is: its name and its files, each with its path
/// under that name (empty for a plain file).
struct Source {
    name: String,
    files: Vec<(Place, String)>,
}

async fn copy(store: &Store, sources: &[String], dest: &str, recursive: bool) -> Result<()> {
    let several = sources.len() > 1;
    let target = remote(dest);
    let mut found = Vec::new();
    for source in sources {
        let source = match remote(source) {
            Some((bucket, key)) => remote_source(store, bucket, key, recursive).await?,
            None if target.is_none() => {
                bail!("{source} and {dest} are both local; name a bucket as <bucket>:")
            }
            None => local_source(Path::new(source), recursive)?,
        };
        found.push(source);
    }
    for source in found {
        for (from, under) in &source.files {
            let to = match &target {
                Some((bucket, key)) => {
                    let into = key.is_empty() || key.ends_with('/') || several;
                    let base = match (into, key.as_str()) {
                        (false, _) => key.trim_end_matches('/').to_string(),
                        (true, "") => source.name.clone(),
                        (true, k) => format!("{}/{}", k.trim_end_matches('/'), source.name),
                    };
                    let key = if under.is_empty() {
                        base
                    } else {
                        format!("{base}/{under}")
                    };
                    Place::Remote(bucket.clone(), key)
                }
                None => {
                    let dest = Path::new(dest);
                    if several {
                        tokio::fs::create_dir_all(dest).await?;
                    }
                    let into = several || dest.is_dir() || dest.to_string_lossy().ends_with('/');
                    let base = if into {
                        dest.join(&source.name)
                    } else {
                        dest.to_path_buf()
                    };
                    Place::Local(if under.is_empty() {
                        base
                    } else {
                        base.join(under)
                    })
                }
            };
            let size = transfer(store, from, &to).await?;
            eprintln!("{from} -> {to} ({})", human(size));
        }
    }
    Ok(())
}

fn local_source(path: &Path, recursive: bool) -> Result<Source> {
    let meta = std::fs::metadata(path).with_context(|| format!("reading {}", path.display()))?;
    if !meta.is_dir() {
        return Ok(Source {
            name: file_name(path)?,
            files: vec![(Place::Local(path.to_path_buf()), String::new())],
        });
    }
    if !recursive {
        bail!("{} is a directory; copy it with -r", path.display());
    }
    // Through the real path, so `.` and `scripts/` have names too.
    let name = file_name(&std::fs::canonicalize(path)?)?;
    let mut files = Vec::new();
    walk(path, "", &mut files)?;
    Ok(Source { name, files })
}

/// The files under `dir`, in order; directories that are symlinks are left
/// out, so a loop can't trap it.
fn walk(dir: &Path, under: &str, files: &mut Vec<(Place, String)>) -> Result<()> {
    let mut entries = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let under = if under.is_empty() {
            name
        } else {
            format!("{under}/{name}")
        };
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            walk(&path, &under, files)?;
        } else if path.is_file() {
            files.push((Place::Local(path), under));
        }
    }
    Ok(())
}

async fn remote_source(
    store: &Store,
    bucket: String,
    key: String,
    recursive: bool,
) -> Result<Source> {
    let name = key
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(&bucket)
        .to_string();
    if !recursive {
        if key.is_empty() || key.ends_with('/') {
            bail!("{bucket}:{key} is a prefix; copy it with -r");
        }
        return Ok(Source {
            name,
            files: vec![(Place::Remote(bucket, key), String::new())],
        });
    }
    let files = match prefix_or_object(store, &bucket, &key).await? {
        Some(keys) => keys
            .into_iter()
            .map(|(k, under)| (Place::Remote(bucket.clone(), k), under))
            .collect(),
        None => vec![(Place::Remote(bucket, key), String::new())],
    };
    Ok(Source { name, files })
}

/// The objects under `key` as a prefix, each with its path under it, or
/// `None` when `key` is an object and nothing is under it.
async fn prefix_or_object(
    store: &Store,
    bucket: &str,
    key: &str,
) -> Result<Option<Vec<(String, String)>>> {
    let prefix = if key.is_empty() || key.ends_with('/') {
        key.to_string()
    } else {
        format!("{key}/")
    };
    let listing = store.list(bucket, &prefix, true).await?;
    let keys: Vec<(String, String)> = listing
        .objects
        .into_iter()
        .filter(|o| !o.key.ends_with('/'))
        .map(|o| {
            let under = o.key[prefix.len()..].to_string();
            (o.key, under)
        })
        .collect();
    if !keys.is_empty() {
        return Ok(Some(keys));
    }
    if key.is_empty() || key.ends_with('/') {
        bail!("nothing under {bucket}:{key}");
    }
    Ok(None)
}

/// Copies one file, and returns its size.
async fn transfer(store: &Store, from: &Place, to: &Place) -> Result<u64> {
    match (from, to) {
        (Place::Remote(bucket, key), Place::Local(dest)) => {
            download(store, bucket, key, dest).await
        }
        (_, Place::Remote(bucket, key)) => {
            let body = match from {
                Place::Local(path) => tokio::fs::read(path)
                    .await
                    .with_context(|| format!("reading {}", path.display()))?,
                Place::Remote(b, k) => {
                    let mut body = Vec::new();
                    store.get(b, k, &mut body).await?;
                    body
                }
            };
            let size = body.len() as u64;
            store.put(bucket, key, body).await?;
            Ok(size)
        }
        (Place::Local(_), Place::Local(_)) => bail!("{from} and {to} are both local"),
    }
}

async fn download(store: &Store, bucket: &str, key: &str, dest: &Path) -> Result<u64> {
    if let Some(dir) = dest.parent().filter(|d| !d.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(dir)
            .await
            .with_context(|| format!("creating {}", dir.display()))?;
    }
    // Into a temporary file first, so a failed download leaves nothing
    // half written under the real name.
    let part = dest.with_extension(match dest.extension() {
        Some(ext) => format!("{}.part", ext.to_string_lossy()),
        None => "part".into(),
    });
    let mut file = tokio::fs::File::create(&part)
        .await
        .with_context(|| format!("creating {}", part.display()))?;
    match store.get(bucket, key, &mut file).await {
        Ok(size) => {
            tokio::fs::rename(&part, dest).await?;
            Ok(size)
        }
        Err(err) => {
            let _ = tokio::fs::remove_file(&part).await;
            Err(err)
        }
    }
}

async fn cat(store: &Store, bucket: &str, key: &str) -> Result<()> {
    store.get(bucket, key, &mut tokio::io::stdout()).await?;
    Ok(())
}

/// A path that names one object.
fn object(path: &str) -> Result<(String, String)> {
    let (bucket, key) = split(path)?;
    if key.is_empty() || key.ends_with('/') {
        bail!("{path} is not an object; use <bucket>:<key>");
    }
    Ok((bucket, key))
}

fn file_name(path: &Path) -> Result<String> {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .with_context(|| format!("{} has no file name", path.display()))
}
