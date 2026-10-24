// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-workspace`: `uc workspace` and `uc ws`, see [`uc::workspace`].

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::json;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use tokio::io::AsyncWriteExt;
use uc::configure::vars::{LoadOptions, Vars};
use uc::kube::cluster::current_context;
use uc::kube::remote::{kubectl, kubectl_json, kubectl_stdin, port_forward};
use uc::kube::roles::registry::{AUTH_SECRET, NAMESPACE as REGISTRY_NAMESPACE};
use uc::kube::var;
use uc::workspace::{self as ws, NAMESPACE, PULL_SECRET, Spec};

/// A dev container of your own on your cluster.
///
/// `up` starts a Debian trixie workspace with your SSH keys and a home
/// that stays; `ssh` gets you in, and `settings` shows how to open it in
/// VS Code. It only needs kubectl to reach the cluster: SSH goes through
/// kubectl port-forward, nothing is exposed.
#[derive(Parser)]
#[command(name = "uc workspace", bin_name = "uc workspace", version, after_help = EXAMPLES)]
struct Cli {
    /// The kubeconfig context [default: the current one].
    #[arg(long, value_name = "NAME", env = "UC_KUBE_CONTEXT", global = true)]
    context: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Make a workspace, or update it and start it again.
    Up {
        /// The workspace [default: dev].
        #[arg(default_value = "dev")]
        name: String,
        /// The image [default: workspace_image, else usecode-base:latest
        /// on registry_host, else on ghcr.io].
        #[arg(long)]
        image: Option<String>,
        /// The home volume's size, when it is made [default:
        /// workspace_size, else 20Gi].
        #[arg(long)]
        size: Option<String>,
        /// The home volume's storage class, when it is made [default:
        /// workspace_storage_class, else the cluster's default].
        #[arg(long)]
        class: Option<String>,
        /// CPUs to reserve, e.g. 2 or 500m.
        #[arg(long)]
        cpu: Option<String>,
        /// Memory to reserve, e.g. 4Gi.
        #[arg(long)]
        memory: Option<String>,
        /// A public key to let in (repeatable) [default: ~/.ssh/*.pub].
        #[arg(long, value_name = "FILE")]
        key: Vec<PathBuf>,
        /// The user in the workspace [default: you, $USER].
        #[arg(long, value_name = "NAME")]
        user: Option<String>,
        /// Set a variable, e.g. -e registry_host=registry.example.com.
        #[arg(short = 'e', long = "extra-var", value_name = "KEY=VALUE")]
        extra_vars: Vec<String>,
        /// The user config, as for `uc kube sync`.
        #[arg(long, value_name = "FILE", env = "UC_CONFIGURE_CONFIG")]
        config: Option<PathBuf>,
    },
    /// The workspaces and whether they run.
    Ls,
    /// A shell in a workspace, or a command (after --).
    Ssh {
        #[arg(default_value = "dev")]
        name: String,
        /// The user to log in as [default: you, $USER].
        #[arg(long, value_name = "NAME")]
        user: Option<String>,
        /// More for ssh, e.g. `-- -L 8080:localhost:8080` or `-- make test`.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// How to reach a workspace with ssh and VS Code.
    Settings {
        #[arg(default_value = "dev")]
        name: String,
        /// The user to log in as [default: you, $USER].
        #[arg(long, value_name = "NAME")]
        user: Option<String>,
        /// Only the ~/.ssh/config entry.
        #[arg(long)]
        ssh_config: bool,
    },
    /// Stop a workspace; its home stays.
    Stop {
        #[arg(default_value = "dev")]
        name: String,
    },
    /// Start a stopped workspace.
    Start {
        #[arg(default_value = "dev")]
        name: String,
    },
    /// Remove a workspace, its home too with --purge.
    Down {
        name: String,
        /// Delete the home volume and everything on it.
        #[arg(long)]
        purge: bool,
    },
    /// The ssh ProxyCommand: stdin and stdout to the workspace's sshd.
    #[command(hide = true)]
    Proxy { name: String },
}

const EXAMPLES: &str = "\
Examples:
  uc ws up                      a workspace called dev, as you, with your ~/.ssh keys
  uc ws up ada --cpu 4 --memory 8Gi --size 50Gi
  ssh dev                       a shell in it (uc ws ssh works, too)
  uc ws ssh -- -L 3000:localhost:3000   with a port brought home
  uc forward ws/dev             every port that listens in it, as it opens
  uc ws settings                ssh and VS Code, step by step
  uc ws stop                    stop it; /home stays for next time
  uc ws down dev --purge        remove it with its home

`uc ws` is short for `uc workspace`. The image is lib/base; push your
own build with `make -C lib/base push`.";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--summary") {
        println!("a dev container of your own on your cluster, over SSH");
        return ExitCode::SUCCESS;
    }
    let cli = Cli::parse();
    let result = tokio::runtime::Runtime::new()
        .map_err(Into::into)
        .and_then(|rt| rt.block_on(run(cli)));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("uc workspace: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    let context = match cli.context {
        Some(c) => c,
        None => current_context()?,
    };
    let name = match &cli.command {
        Command::Up { name, .. }
        | Command::Ssh { name, .. }
        | Command::Settings { name, .. }
        | Command::Stop { name }
        | Command::Start { name }
        | Command::Down { name, .. }
        | Command::Proxy { name } => Some(name.as_str()),
        Command::Ls => None,
    };
    if let Some(name) = name {
        ws::check_name(name)?;
    }
    match cli.command {
        Command::Up {
            name,
            image,
            size,
            class,
            cpu,
            memory,
            key,
            user,
            extra_vars,
            config,
        } => {
            let vars = Vars::load(LoadOptions {
                profile: None,
                extra: extra_vars,
                roles_dir: None,
                config_file: config,
                overlay: vec!["kube".into(), context.clone()],
            })?;
            let registry = var::<String>(&vars, "registry_host")?
                .map(|r| r.trim().trim_end_matches('.').to_lowercase())
                .filter(|r| !r.is_empty());
            let image =
                image
                    .or(var(&vars, "workspace_image")?)
                    .unwrap_or_else(|| match &registry {
                        Some(r) => format!("{r}/usecode-base:latest"),
                        None => "ghcr.io/evgnomon/usecode-base:latest".into(),
                    });
            let pull_from = registry.filter(|r| image.starts_with(&format!("{r}/")));
            let spec = Spec {
                user: ws::local_user(&vars.home, user)?,
                pull_secret: pull_from.is_some(),
                image,
                size: size
                    .or(var(&vars, "workspace_size")?)
                    .unwrap_or_else(|| "20Gi".into()),
                class: class.or(var(&vars, "workspace_storage_class")?),
                cpu,
                memory,
                name,
            };
            let keys = ws::authorized_keys(&vars.home, &key)?;
            up(&context, &spec, &keys, pull_from.as_deref()).await?;
            write_ssh_config(&vars.home, &context, &spec.name, &spec.user.name)
        }
        Command::Ls => ls(&context).await,
        Command::Ssh { name, user, args } => {
            let user = ws::local_user(&home()?, user)?;
            ssh(&context, &name, &user.name, &args)
        }
        Command::Settings {
            name,
            user,
            ssh_config,
        } => {
            let user = ws::local_user(&home()?, user)?;
            let proxy = ws::proxy_command(&exe()?, &context, &name);
            if ssh_config {
                print!("{}", ws::ssh_config(&name, &user.name, &proxy));
            } else {
                print!("{}", ws::settings(&name, &user.name, &context, &proxy));
            }
            Ok(())
        }
        Command::Stop { name } => {
            scale(&context, &name, 0).await?;
            println!("{name} stopped; `uc ws start {name}` brings it back with its home");
            Ok(())
        }
        Command::Start { name } => {
            scale(&context, &name, 1).await?;
            ready(&context, &name).await?;
            println!("{name} is running: ssh {name}");
            Ok(())
        }
        Command::Down { name, purge } => down(&context, &name, purge).await,
        Command::Proxy { name } => proxy(&context, &name).await,
    }
}

/// This program, for the ProxyCommand.
fn exe() -> Result<String> {
    let exe = std::env::current_exe().context("finding uc-workspace")?;
    Ok(exe.to_string_lossy().into_owned())
}

async fn up(context: &str, spec: &Spec, keys: &str, pull_from: Option<&str>) -> Result<()> {
    let obj = ws::object(&spec.name);
    let namespace = json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {"name": NAMESPACE},
    });
    kubectl_stdin(context, &["apply", "-f", "-"], &namespace.to_string()).await?;
    if let Some(registry) = pull_from {
        pull_secret(context, registry).await?;
    }
    let have = kubectl(
        context,
        &[
            "-n",
            NAMESPACE,
            "get",
            "pvc",
            &obj,
            "--ignore-not-found",
            "-o",
            "name",
        ],
    )
    .await?;
    if have.trim().is_empty() {
        let claim = ws::claim(spec).to_string();
        kubectl_stdin(context, &["create", "-f", "-"], &claim).await?;
    }
    let list = json!({"apiVersion": "v1", "kind": "List", "items": ws::manifests(spec, keys)});
    kubectl_stdin(context, &["apply", "-f", "-"], &list.to_string()).await?;
    // An `up` on a stopped workspace starts it, too.
    scale(context, &spec.name, 1).await?;
    eprintln!("waiting for {} ({}) on {context}...", spec.name, spec.image);
    ready(context, &spec.name).await?;
    println!(
        "{name} is running.\n  ssh {name}              a shell in it\n  uc ws settings {name}   ssh config and VS Code",
        name = spec.name
    );
    Ok(())
}

/// Puts the workspace's entry in `~/.ssh/config.d`, so `ssh <name>` and
/// VS Code find it, and says so when `~/.ssh/config` doesn't read it.
fn write_ssh_config(home: &Path, context: &str, name: &str, user: &str) -> Result<()> {
    let path = ws::ssh_config_path(home, name);
    let entry = ws::ssh_config(name, user, &ws::proxy_command(&exe()?, context, name));
    let dir = path.parent().context("no ~/.ssh/config.d")?;
    std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    std::fs::write(&path, entry).with_context(|| format!("writing {}", path.display()))?;
    let config = home.join(".ssh/config");
    let text = std::fs::read_to_string(&config).unwrap_or_default();
    if !ws::includes_config_d(&text) {
        eprintln!(
            "{} has the ssh entry; add `Include config.d/*` at the top of {} for `ssh {name}`",
            path.display(),
            config.display()
        );
    }
    Ok(())
}

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")
}

/// Copies the registry's login into workspace/registry, as the
/// `.dockerconfigjson` the pods pull with, never echoing it.
async fn pull_secret(context: &str, registry: &str) -> Result<()> {
    use base64::Engine;
    let b64 = &base64::engine::general_purpose::STANDARD;
    let field = |key: &'static str| async move {
        let out = kubectl(
            context,
            &[
                "-n",
                REGISTRY_NAMESPACE,
                "get",
                "secret",
                AUTH_SECRET,
                "--ignore-not-found",
                "-o",
                &format!("jsonpath={{.data.{key}}}"),
            ],
        )
        .await?;
        let value = b64
            .decode(out.trim())
            .context("reading the registry login")?;
        Ok::<_, anyhow::Error>(String::from_utf8_lossy(&value).into_owned())
    };
    let (user, password) = (field("username").await?, field("password").await?);
    if user.is_empty() || password.is_empty() {
        bail!(
            "{REGISTRY_NAMESPACE}/{AUTH_SECRET} has no login yet: run `uc kube sync -t registry` \
             first, or pass --image with one the cluster can pull without a login"
        );
    }
    let auth = b64.encode(format!("{user}:{password}"));
    let config =
        json!({"auths": {registry: {"username": user, "password": password, "auth": auth}}});
    let secret = json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "type": "kubernetes.io/dockerconfigjson",
        "metadata": {"name": PULL_SECRET, "namespace": NAMESPACE},
        "stringData": {".dockerconfigjson": config.to_string()},
    });
    // `create` or `replace`, never `apply`, which would keep a copy of the
    // password in the last-applied annotation.
    let have = kubectl(
        context,
        &[
            "-n",
            NAMESPACE,
            "get",
            "secret",
            PULL_SECRET,
            "--ignore-not-found",
            "-o",
            "name",
        ],
    )
    .await?;
    let verb = if have.trim().is_empty() {
        "create"
    } else {
        "replace"
    };
    kubectl_stdin(context, &[verb, "-f", "-"], &secret.to_string()).await?;
    Ok(())
}

async fn scale(context: &str, name: &str, replicas: u32) -> Result<()> {
    kubectl(
        context,
        &[
            "-n",
            NAMESPACE,
            "scale",
            &format!("deployment/{}", ws::object(name)),
            &format!("--replicas={replicas}"),
        ],
    )
    .await
    .with_context(|| format!("no workspace {name}? `uc ws ls` lists them"))?;
    Ok(())
}

/// Waits for the workspace's pod to take connections.
async fn ready(context: &str, name: &str) -> Result<()> {
    kubectl(
        context,
        &[
            "-n",
            NAMESPACE,
            "rollout",
            "status",
            &format!("deployment/{}", ws::object(name)),
            "--timeout=5m",
        ],
    )
    .await
    .with_context(|| {
        format!(
            "{name} did not come up; `kubectl -n {NAMESPACE} describe pod -l {}={name}` says why",
            ws::LABEL
        )
    })?;
    Ok(())
}

async fn ls(context: &str) -> Result<()> {
    let list = kubectl_json(context, &["-n", NAMESPACE, "get", "deployments"]).await?;
    let rows = ws::rows(&list);
    if rows.is_empty() {
        println!("no workspaces on {context}; `uc ws up` makes one");
        return Ok(());
    }
    let width = rows.iter().map(|r| r.name.len()).max().unwrap_or(0);
    for r in &rows {
        println!("{:width$}  {:8}  {}", r.name, r.state, r.image);
    }
    Ok(())
}

/// Replaces this process with ssh to the workspace.
fn ssh(context: &str, name: &str, user: &str, args: &[String]) -> Result<()> {
    let proxy = ws::proxy_command(&exe()?, context, name);
    let err = std::process::Command::new("ssh")
        .args(["-o", &format!("ProxyCommand={proxy}")])
        .args(["-o", "StrictHostKeyChecking=accept-new"])
        .args(["-o", "LogLevel=ERROR"])
        .args(["-o", "ServerAliveInterval=30"])
        .args(["-l", user])
        .args(args)
        .arg(ws::host_alias(name))
        .exec();
    bail!("running ssh: {err}")
}

async fn down(context: &str, name: &str, purge: bool) -> Result<()> {
    let obj = ws::object(name);
    let mut kinds = String::from("deployment,service,configmap");
    if purge {
        kinds += ",persistentvolumeclaim";
    }
    kubectl(
        context,
        &[
            "-n",
            NAMESPACE,
            "delete",
            &kinds,
            &obj,
            "--ignore-not-found",
            "--wait",
        ],
    )
    .await?;
    let entry = ws::ssh_config_path(&home()?, name);
    match std::fs::remove_file(&entry) {
        Err(err) if err.kind() != std::io::ErrorKind::NotFound => {
            eprintln!("could not remove {}: {err}", entry.display())
        }
        _ => {}
    }
    if purge {
        println!("{name} and its home are gone");
    } else {
        println!(
            "{name} is gone; its home stays in pvc/{obj} for the next `uc ws up {name}` (--purge deletes it)"
        );
    }
    Ok(())
}

/// What ssh runs as its ProxyCommand: a port-forward to the workspace's
/// sshd, with this process's stdin and stdout on the other end.
async fn proxy(context: &str, name: &str) -> Result<()> {
    let obj = ws::object(name);
    let deploy = kubectl_json(context, &["-n", NAMESPACE, "get", "deployment", &obj])
        .await
        .with_context(|| format!("no workspace {name} on {context}; `uc ws up {name}` makes it"))?;
    if deploy["spec"]["replicas"].as_u64() == Some(0) {
        bail!("{name} is stopped; `uc ws start {name}` starts it");
    }
    ready(context, name).await?;
    let forward = port_forward(context, NAMESPACE, &format!("svc/{obj}"), ws::SSH_PORT).await?;
    let addr = forward.url.trim_start_matches("http://").to_string();
    let stream = tokio::net::TcpStream::connect(&addr)
        .await
        .with_context(|| format!("connecting to {name}'s sshd"))?;
    let (mut from, mut to) = stream.into_split();
    let up = async {
        let _ = tokio::io::copy(&mut tokio::io::stdin(), &mut to).await;
        let _ = to.shutdown().await;
        std::future::pending::<()>().await
    };
    let down = async {
        let mut stdout = tokio::io::stdout();
        let _ = tokio::io::copy(&mut from, &mut stdout).await;
        let _ = stdout.flush().await;
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
    }
    // The stdin reader blocks a thread the runtime would wait on, so end
    // here, after ending kubectl.
    drop(forward);
    std::process::exit(0)
}
