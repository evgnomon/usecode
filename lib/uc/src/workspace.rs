// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc workspace` (`uc ws`): a dev container of your own on the cluster,
//! the Debian trixie image from `lib/base`, reached over SSH as you: the
//! same login name and uid you have on this machine, with sudo.
//!
//! Each workspace is a Deployment `ws-<name>` in the `workspace`
//! namespace with one pod, its home on a volume of its own, so stopping,
//! starting or a new image keeps what is in `/home`. Your public keys go
//! in a ConfigMap; a change to them restarts the pod to pick them up.
//!
//! Nothing is open to the outside: SSH goes through
//! `kubectl port-forward` (`uc workspace proxy`, the ProxyCommand), so
//! whoever can reach the cluster with kubectl can reach their workspace
//! and no one else.

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const NAMESPACE: &str = "workspace";
/// The login to `registry_host` the pods pull the image with.
pub const PULL_SECRET: &str = "registry";
/// The label every workspace object carries, with the workspace's name.
pub const LABEL: &str = "usecode.dev/workspace";
/// The annotation on the pods with the fingerprint of the keys.
const KEYS_FINGERPRINT: &str = "usecode.dev/keys-fingerprint";
pub const SSH_PORT: u16 = 22;

/// Who you are in a workspace: the image's entrypoint makes this user.
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    pub name: String,
    pub uid: u32,
}

/// You on this machine: `name`, else `$USER` (or `$LOGNAME`), with the
/// uid that owns `home`, so files you copy back and forth keep their owner.
pub fn local_user(home: &Path, name: Option<String>) -> Result<User> {
    use std::os::unix::fs::MetadataExt;
    let name = name
        .or_else(|| std::env::var("USER").ok())
        .or_else(|| std::env::var("LOGNAME").ok())
        .filter(|n| !n.is_empty())
        .context("USER is not set; pass --user")?;
    check_user(&name)?;
    let uid = std::fs::metadata(home)
        .with_context(|| format!("reading {}", home.display()))?
        .uid();
    // root's home on the laptop is no reason to be uid 0 in there.
    let uid = if uid == 0 { 1000 } else { uid };
    Ok(User { name, uid })
}

/// A login name useradd takes, and not root.
pub fn check_user(name: &str) -> Result<()> {
    let ok = name.len() <= 32
        && name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_lowercase() || b == b'_')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        && name != "root";
    if !ok {
        bail!(
            "'{name}' can't be the workspace user: pass --user with up to 32 lowercase letters, digits, _ and -"
        );
    }
    Ok(())
}

/// What `uc workspace up` makes.
#[derive(Debug, Clone)]
pub struct Spec {
    pub name: String,
    pub image: String,
    pub user: User,
    /// Whether the pods pull with [`PULL_SECRET`].
    pub pull_secret: bool,
    /// The home volume's size, e.g. `20Gi`.
    pub size: String,
    /// The storage class of the volume [default: the cluster's default].
    pub class: Option<String>,
    pub cpu: Option<String>,
    pub memory: Option<String>,
}

/// The name of a workspace's Deployment, Service, ConfigMap and volume.
pub fn object(name: &str) -> String {
    format!("ws-{name}")
}

/// The host name ssh and VS Code know the workspace by.
pub fn host_alias(name: &str) -> String {
    format!("uc-ws-{name}")
}

/// A workspace name has to make a DNS label once `ws-` is in front.
pub fn check_name(name: &str) -> Result<()> {
    let ok = !name.is_empty()
        && name.len() <= 50
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
        && !name.ends_with('-');
    if !ok {
        bail!(
            "'{name}' can't be a workspace name: use up to 50 lowercase letters, digits and dashes"
        );
    }
    Ok(())
}

/// The public keys in `~/.ssh/*.pub`, or in `files` when given, one per
/// line.
pub fn authorized_keys(home: &Path, files: &[std::path::PathBuf]) -> Result<String> {
    let files = if files.is_empty() {
        let dir = home.join(".ssh");
        let mut found: Vec<_> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "pub"))
            .collect();
        found.sort();
        if found.is_empty() {
            bail!(
                "no public key in {}; make one with `ssh-keygen -t ed25519` or pass --key",
                dir.display()
            );
        }
        found
    } else {
        files.to_vec()
    };
    let mut keys = String::new();
    for file in &files {
        let text =
            std::fs::read_to_string(file).with_context(|| format!("reading {}", file.display()))?;
        for line in text.lines().map(str::trim) {
            if !line.is_empty() && !line.starts_with('#') {
                keys += line;
                keys.push('\n');
            }
        }
    }
    if keys.is_empty() {
        bail!("no keys in the public key files");
    }
    Ok(keys)
}

fn fingerprint(keys: &str) -> String {
    let hash = Sha256::digest(keys.as_bytes());
    hash[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn labels(name: &str) -> Value {
    json!({"app.kubernetes.io/name": "workspace", LABEL: name})
}

/// Everything `kubectl apply` keeps for a workspace, the volume aside,
/// which [`claim`] makes once.
pub fn manifests(spec: &Spec, keys: &str) -> Vec<Value> {
    let obj = object(&spec.name);
    let labels = labels(&spec.name);
    let keys_map = json!({
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {"name": obj, "namespace": NAMESPACE, "labels": labels},
        "data": {"authorized_keys": keys},
    });
    let mut requests = serde_json::Map::new();
    if let Some(cpu) = &spec.cpu {
        requests.insert("cpu".into(), json!(cpu));
    }
    if let Some(memory) = &spec.memory {
        requests.insert("memory".into(), json!(memory));
    }
    let mut pod = json!({
        "terminationGracePeriodSeconds": 10,
        "containers": [{
            "name": "workspace",
            "image": spec.image,
            "imagePullPolicy": if spec.image.ends_with(":latest") { "Always" } else { "IfNotPresent" },
            "env": [
                {"name": "USECODE_USER", "value": spec.user.name},
                {"name": "USECODE_UID", "value": spec.user.uid.to_string()},
            ],
            "ports": [{"name": "ssh", "containerPort": SSH_PORT}],
            "resources": {"requests": requests},
            "readinessProbe": {"tcpSocket": {"port": SSH_PORT}, "periodSeconds": 5},
            "volumeMounts": [
                {"name": "home", "mountPath": "/home"},
                {"name": "keys", "mountPath": "/etc/usecode", "readOnly": true},
            ],
        }],
        "volumes": [
            {"name": "home", "persistentVolumeClaim": {"claimName": obj}},
            {"name": "keys", "configMap": {"name": obj}},
        ],
    });
    if spec.pull_secret {
        pod["imagePullSecrets"] = json!([{"name": PULL_SECRET}]);
    }
    let deployment = json!({
        "apiVersion": "apps/v1",
        "kind": "Deployment",
        "metadata": {"name": obj, "namespace": NAMESPACE, "labels": labels},
        "spec": {
            "replicas": 1,
            // One volume, one pod: the old one goes before the new one starts.
            "strategy": {"type": "Recreate"},
            "selector": {"matchLabels": {LABEL: spec.name}},
            "template": {
                "metadata": {
                    "labels": labels,
                    "annotations": {KEYS_FINGERPRINT: fingerprint(keys)},
                },
                "spec": pod,
            },
        },
    });
    let service = json!({
        "apiVersion": "v1",
        "kind": "Service",
        "metadata": {"name": obj, "namespace": NAMESPACE, "labels": labels},
        "spec": {
            "selector": {LABEL: spec.name},
            "ports": [{"name": "ssh", "port": SSH_PORT, "targetPort": "ssh"}],
        },
    });
    vec![keys_map, deployment, service]
}

/// The home volume of a workspace.
pub fn claim(spec: &Spec) -> Value {
    let mut claim = json!({
        "apiVersion": "v1",
        "kind": "PersistentVolumeClaim",
        "metadata": {"name": object(&spec.name), "namespace": NAMESPACE, "labels": labels(&spec.name)},
        "spec": {
            "accessModes": ["ReadWriteOnce"],
            "resources": {"requests": {"storage": spec.size}},
        },
    });
    if let Some(class) = &spec.class {
        claim["spec"]["storageClassName"] = json!(class);
    }
    claim
}

/// `word` as one word for sh, quoted only when it has to be.
pub fn sh_quote(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:@=+,".contains(&b));
    if plain {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// The ProxyCommand that reaches workspace `name` on `context`.
pub fn proxy_command(exe: &str, context: &str, name: &str) -> String {
    format!(
        "{} proxy --context {} {}",
        sh_quote(exe),
        sh_quote(context),
        sh_quote(name)
    )
}

/// The `~/.ssh/config` entry for workspace `name`: `ssh <name>` and
/// `ssh uc-ws-<name>` both reach it. The way in is the cluster's API over
/// TLS, so the workspace's host key is taken quietly the first time and
/// checked from then on.
pub fn ssh_config(name: &str, user: &str, proxy: &str) -> String {
    format!(
        "Host {name} {alias}\n  \
           HostName {alias}\n  \
           User {user}\n  \
           ProxyCommand {proxy}\n  \
           StrictHostKeyChecking accept-new\n  \
           LogLevel ERROR\n  \
           ServerAliveInterval 30\n",
        alias = host_alias(name)
    )
}

/// Where `uc workspace up` writes the entry, for `Include config.d/*` in
/// `~/.ssh/config`.
pub fn ssh_config_path(home: &Path, name: &str) -> std::path::PathBuf {
    home.join(".ssh/config.d")
        .join(format!("{}.conf", host_alias(name)))
}

/// Whether `~/.ssh/config` reads `~/.ssh/config.d`.
pub fn includes_config_d(config: &str) -> bool {
    config.lines().any(|l| {
        let mut words = l.split_whitespace();
        words
            .next()
            .is_some_and(|w| w.eq_ignore_ascii_case("include"))
            && words.any(|w| w.contains("config.d/"))
    })
}

/// What `uc workspace settings` prints: the ssh entry and how to open the
/// workspace in VS Code with it.
pub fn settings(name: &str, user: &str, context: &str, proxy: &str) -> String {
    let alias = host_alias(name);
    let entry = ssh_config(name, user, proxy);
    format!(
        "\
Workspace {name} on {context}

1. SSH

   `uc workspace up` puts this in ~/.ssh/config.d/{alias}.conf, which
   ~/.ssh/config reads with `Include config.d/*`:

{entry}
   Then `ssh {name}` gets you a shell as {user}, the same name you
   have here, with sudo. scp, rsync and
   `ssh -L 8080:localhost:8080 {name}` work as you'd expect.
   `uc forward ws/{name}` brings home whatever listens in it, as it
   opens, without ssh.
   `uc workspace ssh {name}` does the same without the config.

2. VS Code

   a. Install the \"Remote - SSH\" extension (ms-vscode-remote.remote-ssh):
        code --install-extension ms-vscode-remote.remote-ssh
   b. Command Palette (Ctrl+Shift+P) > \"Remote-SSH: Connect to Host...\"
      > {name}. VS Code puts its server in the workspace and you're
      working inside the dev container.
   c. File > Open Folder > /home/{user} (or a repository you cloned there).
      Extensions you install now install in the workspace.

   Or straight from a terminal:
        code --remote ssh-remote+{name} /home/{user}

   The workspace is the dev container: the terminal, the debugger and
   the language servers all run there, on the cluster, not on your laptop.
   Extensions you always want there go in your VS Code settings:
        \"remote.SSH.defaultExtensions\": [\"rust-lang.rust-analyzer\"]

Good to know

   - /home is the workspace's own volume: it stays when you stop, start or
     update the workspace. The rest of the container is new on each start,
     so install tools with sudo apt again, or keep them under ~.
   - `uc workspace stop {name}` frees the cluster while you're away;
     ssh says so if you connect to a stopped one.
   - Your keys are the ones from ~/.ssh/*.pub at `uc workspace up`; run it
     again after adding one.
"
    )
}

/// One row of `uc workspace ls`, from a Deployment.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub name: String,
    pub state: String,
    pub image: String,
}

/// The workspaces in a `kubectl get deploy -o json` list.
pub fn rows(list: &Value) -> Vec<Row> {
    let mut rows: Vec<Row> = list["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|d| {
            let name = d["metadata"]["labels"][LABEL].as_str()?.to_string();
            let want = d["spec"]["replicas"].as_u64().unwrap_or(1);
            let ready = d["status"]["readyReplicas"].as_u64().unwrap_or(0);
            let state = match (want, ready) {
                (0, _) => "stopped",
                (_, 0) => "starting",
                _ => "running",
            };
            let image = d["spec"]["template"]["spec"]["containers"][0]["image"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            Some(Row {
                name,
                state: state.into(),
                image,
            })
        })
        .collect();
    rows.sort_by(|a, b| a.name.cmp(&b.name));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        Spec {
            name: "ada".into(),
            image: "registry.example.com/usecode-base:latest".into(),
            user: User {
                name: "hg".into(),
                uid: 1001,
            },
            pull_secret: true,
            size: "20Gi".into(),
            class: None,
            cpu: Some("2".into()),
            memory: None,
        }
    }

    #[test]
    fn names() {
        assert!(check_name("ada").is_ok());
        assert!(check_name("ada-2").is_ok());
        for bad in ["", "Ada", "-a", "a-", "a_b", &"a".repeat(51)] {
            assert!(check_name(bad).is_err(), "{bad}");
        }
        assert_eq!(object("ada"), "ws-ada");
        assert_eq!(host_alias("ada"), "uc-ws-ada");
    }

    #[test]
    fn makes_the_objects() {
        let m = manifests(&spec(), "ssh-ed25519 AAAA ada\n");
        let kinds: Vec<_> = m.iter().map(|o| o["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["ConfigMap", "Deployment", "Service"]);
        assert_eq!(m[0]["data"]["authorized_keys"], "ssh-ed25519 AAAA ada\n");
        let pod = &m[1]["spec"]["template"]["spec"];
        assert_eq!(pod["imagePullSecrets"][0]["name"], "registry");
        assert_eq!(pod["containers"][0]["imagePullPolicy"], "Always");
        assert_eq!(pod["containers"][0]["resources"]["requests"]["cpu"], "2");
        let env = &pod["containers"][0]["env"];
        assert_eq!(env[0], json!({"name": "USECODE_USER", "value": "hg"}));
        assert_eq!(env[1], json!({"name": "USECODE_UID", "value": "1001"}));
        assert_eq!(
            pod["volumes"][0]["persistentVolumeClaim"]["claimName"],
            "ws-ada"
        );
        assert_eq!(m[1]["spec"]["strategy"]["type"], "Recreate");
        assert_eq!(m[2]["spec"]["selector"][LABEL], "ada");
        assert!(claim(&spec())["spec"].get("storageClassName").is_none());
    }

    #[test]
    fn new_keys_restart_the_pod() {
        let a = manifests(&spec(), "ssh-ed25519 A\n");
        let b = manifests(&spec(), "ssh-ed25519 B\n");
        let fp = |m: &[Value]| m[1]["spec"]["template"]["metadata"]["annotations"].clone();
        assert_ne!(fp(&a), fp(&b));
    }

    #[test]
    fn reads_the_public_keys() {
        let dir = std::env::temp_dir().join(format!("uc-ws-keys-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".ssh")).unwrap();
        assert!(authorized_keys(&dir, &[]).is_err());
        std::fs::write(dir.join(".ssh/id_ed25519.pub"), "ssh-ed25519 AAAA ada\n\n").unwrap();
        std::fs::write(dir.join(".ssh/id_ed25519"), "private").unwrap();
        assert_eq!(
            authorized_keys(&dir, &[]).unwrap(),
            "ssh-ed25519 AAAA ada\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn users() {
        for good in ["hg", "evgnomon", "_ci", "a-b_1"] {
            assert!(check_user(good).is_ok(), "{good}");
        }
        for bad in ["", "root", "Hamed", "1a", "a.b", "-a", &"a".repeat(33)] {
            assert!(check_user(bad).is_err(), "{bad}");
        }
        let user = local_user(&std::env::temp_dir(), Some("hg".into())).unwrap();
        assert_eq!(user.name, "hg");
        assert!(local_user(&std::env::temp_dir(), Some("root".into())).is_err());
    }

    #[test]
    fn quotes_for_the_proxy_command() {
        assert_eq!(
            proxy_command("/usr/local/bin/uc-workspace", "cp-1", "ada"),
            "/usr/local/bin/uc-workspace proxy --context cp-1 ada"
        );
        assert_eq!(sh_quote("my ctx"), "'my ctx'");
        assert_eq!(sh_quote("it's"), r"'it'\''s'");
        let entry = ssh_config("ada", "hg", "uc-workspace proxy --context cp-1 ada");
        assert!(entry.starts_with("Host ada uc-ws-ada\n  HostName uc-ws-ada\n  User hg\n"));
        let text = settings("ada", "hg", "cp-1", "p");
        assert!(text.contains("ssh-remote+ada /home/hg"));
        assert_eq!(
            ssh_config_path(Path::new("/h"), "ada"),
            Path::new("/h/.ssh/config.d/uc-ws-ada.conf")
        );
        assert!(includes_config_d("# x\nInclude config.d/*\n"));
        assert!(includes_config_d("include ~/.ssh/config.d/*.conf"));
        assert!(!includes_config_d("Host a\n  HostName config.d/\n"));
    }

    #[test]
    fn lists_the_workspaces() {
        let list = json!({"items": [
            {"metadata": {"labels": {LABEL: "bob"}}, "spec": {"replicas": 0,
              "template": {"spec": {"containers": [{"image": "i"}]}}}, "status": {}},
            {"metadata": {"labels": {LABEL: "ada"}}, "spec": {"replicas": 1,
              "template": {"spec": {"containers": [{"image": "i"}]}}}, "status": {"readyReplicas": 1}},
            {"metadata": {"labels": {}}, "spec": {}},
        ]});
        let r = rows(&list);
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].name.as_str(), r[0].state.as_str()),
            ("ada", "running")
        );
        assert_eq!(r[1].state, "stopped");
    }
}
