// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc forward`: the services of a namespace on localhost, with
//! `kubectl port-forward`, one per Service, started again when it drops
//! (a pod restarting ends it).
//!
//! Each port is forwarded to the same port on localhost when that one is
//! free, so the links stay the same from one run to the next; ports below
//! 1024 go to 10000 + the port, which needs no root. A taken one gets any
//! free port.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::net::TcpListener;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

/// One port of a Service to forward.
#[derive(Debug, Clone, PartialEq)]
pub struct Port {
    pub port: u16,
    /// The Service's name for it, maybe empty.
    pub name: String,
    pub local: u16,
}

/// A Service and the ports of it to forward.
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    pub namespace: String,
    pub name: String,
    pub ports: Vec<Port>,
}

/// What a known port is, and where on it to start.
struct Hint {
    namespace: &'static str,
    service: &'static str,
    port: u16,
    what: &'static str,
    path: &'static str,
}

const HINTS: &[Hint] = &[
    Hint {
        namespace: "obs",
        service: "vmsingle",
        port: 8428,
        what: "metrics and dashboards (vmui)",
        path: "/vmui/",
    },
    Hint {
        namespace: "obs",
        service: "vlsingle",
        port: 9428,
        what: "logs (vmui)",
        path: "/select/vmui/",
    },
    Hint {
        namespace: "obs",
        service: "vmalert",
        port: 8880,
        what: "alerts",
        path: "/",
    },
    Hint {
        namespace: "obs",
        service: "kube-state-metrics",
        port: 8080,
        what: "kube-state-metrics",
        path: "/metrics",
    },
    Hint {
        namespace: "obs",
        service: "node-exporter",
        port: 9100,
        what: "node-exporter (one node)",
        path: "/metrics",
    },
    Hint {
        namespace: "rustfs",
        service: "rustfs",
        port: 9000,
        what: "S3 API",
        path: "",
    },
    Hint {
        namespace: "rustfs",
        service: "rustfs",
        port: 9001,
        what: "console",
        path: "/rustfs/console/",
    },
    Hint {
        namespace: "mail",
        service: "stalwart",
        port: 8080,
        what: "Stalwart admin",
        path: "/",
    },
    Hint {
        namespace: "forgejo",
        service: "forgejo-http",
        port: 3000,
        what: "Forgejo",
        path: "/",
    },
    Hint {
        namespace: "registry",
        service: "registry",
        port: 5000,
        what: "registry API",
        path: "/v2/_catalog",
    },
];

/// Port names that are not spoken over HTTP.
const NOT_HTTP: &[&str] = &[
    "ssh",
    "smtp",
    "submission",
    "submissions",
    "imap",
    "imaps",
    "pop3",
    "pop3s",
    "postgres",
    "postgresql",
    "mysql",
    "redis",
    "dns",
    "dns-tcp",
    "grpc",
];

/// The Services of `namespace` with TCP ports, `only` those named when it
/// isn't empty; each with `port` only, when one is given.
pub fn services(
    namespace: &str,
    list: &Value,
    only: &[(String, Option<u16>)],
) -> Result<Vec<Service>> {
    let mut found = Vec::new();
    for item in list["items"].as_array().into_iter().flatten() {
        let name = item["metadata"]["name"].as_str().unwrap_or_default();
        let wanted = only.iter().find(|(n, _)| n == name);
        if !only.is_empty() && wanted.is_none() {
            continue;
        }
        // A Service without pods behind it has nothing to forward to.
        if item["spec"]["selector"]
            .as_object()
            .is_none_or(|s| s.is_empty())
        {
            continue;
        }
        let ports: Vec<Port> = item["spec"]["ports"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|p| p["protocol"].as_str().unwrap_or("TCP") == "TCP")
            .filter_map(|p| {
                let port = u16::try_from(p["port"].as_u64()?).ok()?;
                Some(Port {
                    port,
                    name: p["name"].as_str().unwrap_or_default().to_string(),
                    local: 0,
                })
            })
            .filter(|p| {
                wanted
                    .and_then(|(_, port)| *port)
                    .is_none_or(|w| w == p.port)
            })
            .collect();
        if !ports.is_empty() {
            found.push(Service {
                namespace: namespace.to_string(),
                name: name.to_string(),
                ports,
            });
        }
    }
    for (name, port) in only {
        let Some(service) = found.iter().find(|s| &s.name == name) else {
            bail!("{namespace} has no Service {name} with a TCP port to forward");
        };
        if let Some(port) = port
            && !service.ports.iter().any(|p| p.port == *port)
        {
            bail!("{namespace}/{name} has no TCP port {port}");
        }
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}

/// `svc`, `svc:port` or a bare port number off `namespace/...`.
pub fn parse_pick(pick: &str) -> Result<(String, Option<u16>)> {
    match pick.rsplit_once(':') {
        Some((name, port)) => Ok((
            name.to_string(),
            Some(
                port.parse()
                    .with_context(|| format!("'{port}' is not a port"))?,
            ),
        )),
        None => Ok((pick.to_string(), None)),
    }
}

/// The local port to ask for first.
pub fn preferred(port: u16) -> u16 {
    if port < 1024 { port + 10_000 } else { port }
}

/// Gives every port a free local one, `preferred` when it can.
pub fn assign(services: &mut [Service], free: impl Fn(u16) -> bool) -> Result<()> {
    let mut taken = Vec::new();
    for service in services.iter_mut() {
        for p in &mut service.ports {
            let want = preferred(p.port);
            p.local = if !taken.contains(&want) && free(want) {
                want
            } else {
                any_free_port()?
            };
            taken.push(p.local);
        }
    }
    Ok(())
}

pub fn is_free(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn any_free_port() -> Result<u16> {
    Ok(TcpListener::bind(("127.0.0.1", 0))
        .context("finding a free local port")?
        .local_addr()?
        .port())
}

/// What the port is, and how to reach it on localhost.
pub fn describe(service: &Service, port: &Port) -> (String, String) {
    let hint = HINTS.iter().find(|h| {
        h.namespace == service.namespace && h.service == service.name && h.port == port.port
    });
    let what = match hint {
        Some(h) => h.what.to_string(),
        None if port.name.is_empty() => String::new(),
        None => port.name.clone(),
    };
    let http = hint.is_some() || !NOT_HTTP.contains(&port.name.as_str());
    let at = if http {
        format!(
            "http://localhost:{}{}",
            port.local,
            hint.map_or("/", |h| h.path)
        )
    } else {
        format!("localhost:{}", port.local)
    };
    (what, at)
}

/// Forwards `service` until the process ends, starting kubectl again
/// whenever it stops.
pub async fn keep_forwarding(context: String, service: Service) {
    let label = format!("{}/{}", service.namespace, service.name);
    let mut args = vec![
        "--context".to_string(),
        context,
        "-n".into(),
        service.namespace.clone(),
        "port-forward".into(),
        format!("svc/{}", service.name),
    ];
    args.extend(
        service
            .ports
            .iter()
            .map(|p| format!("{}:{}", p.local, p.port)),
    );
    let mut up_before = false;
    loop {
        let why = forward_once(&args, &label, &mut up_before).await;
        eprintln!("{label}: {why}; again in 3s");
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// One run of kubectl port-forward, and why it ended.
async fn forward_once(args: &[String], label: &str, up_before: &mut bool) -> String {
    let child = Command::new("kubectl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(err) => return format!("running kubectl: {err}"),
    };
    let mut out = BufReader::new(child.stdout.take().expect("stdout is piped")).lines();
    let mut err = BufReader::new(child.stderr.take().expect("stderr is piped")).lines();
    let mut last_error = String::new();
    loop {
        tokio::select! {
            line = out.next_line() => match line {
                Ok(Some(line)) => {
                    // "Forwarding from 127.0.0.1:8428 -> 8428", once per port.
                    if line.starts_with("Forwarding from 127.0.0.1") && *up_before {
                        eprintln!("{label}: forwarding again");
                        *up_before = false;
                    }
                }
                _ => break,
            },
            line = err.next_line() => match line {
                Ok(Some(line)) if !line.trim().is_empty() => last_error = line.trim().to_string(),
                Ok(Some(_)) => {}
                _ => break,
            },
        }
    }
    let status = child.wait().await;
    *up_before = true;
    match (last_error.is_empty(), status) {
        (false, _) => format!("kubectl port-forward stopped: {last_error}"),
        (true, Ok(s)) => format!("kubectl port-forward stopped ({s})"),
        (true, Err(e)) => format!("kubectl port-forward stopped: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list() -> Value {
        json!({"items": [
            {"metadata": {"name": "vmsingle"}, "spec": {"selector": {"app": "vm"}, "ports": [
                {"name": "http", "port": 8428, "protocol": "TCP"}]}},
            {"metadata": {"name": "pdns"}, "spec": {"selector": {"app": "pdns"}, "ports": [
                {"name": "dns", "port": 53, "protocol": "UDP"},
                {"name": "dns-tcp", "port": 53, "protocol": "TCP"}]}},
            {"metadata": {"name": "external"}, "spec": {"ports": [{"port": 443}]}},
            {"metadata": {"name": "mail"}, "spec": {"selector": {"app": "mail"}, "ports": [
                {"name": "smtp", "port": 25}, {"name": "imaps", "port": 993}]}},
        ]})
    }

    #[test]
    fn forwards_tcp_ports_of_services_with_pods() {
        let found = services("obs", &list(), &[]).unwrap();
        let names: Vec<_> = found.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["mail", "pdns", "vmsingle"]);
        // UDP is left out.
        assert_eq!(found[1].ports.len(), 1);
        assert_eq!(found[1].ports[0].name, "dns-tcp");
    }

    #[test]
    fn picks_one_service_or_port() {
        let found = services("mail", &list(), &[parse_pick("mail:993").unwrap()]).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].ports.len(), 1);
        assert_eq!(found[0].ports[0].port, 993);
        assert!(services("mail", &list(), &[parse_pick("nope").unwrap()]).is_err());
        assert!(services("mail", &list(), &[parse_pick("mail:80").unwrap()]).is_err());
        assert!(parse_pick("mail:x").is_err());
    }

    #[test]
    fn keeps_ports_where_it_can() {
        let mut found = services("mail", &list(), &[]).unwrap();
        assign(&mut found, |p| p != 8428).unwrap();
        let mail = &found[0].ports;
        // Below 1024 needs root, so 10000 is added.
        assert_eq!((mail[0].local, mail[1].local), (10_025, 10_993));
        // 8428 is taken here, so another one.
        assert_ne!(found[2].ports[0].local, 8428);
    }

    #[test]
    fn links_to_the_known_ones() {
        let mut found = services("obs", &list(), &[parse_pick("vmsingle").unwrap()]).unwrap();
        assign(&mut found, |_| true).unwrap();
        let (what, at) = describe(&found[0], &found[0].ports[0]);
        assert_eq!(what, "metrics and dashboards (vmui)");
        assert_eq!(at, "http://localhost:8428/vmui/");
        let mut mail = services("mail", &list(), &[]).unwrap();
        assign(&mut mail, |_| true).unwrap();
        assert_eq!(describe(&mail[0], &mail[0].ports[0]).1, "localhost:10025");
    }
}
