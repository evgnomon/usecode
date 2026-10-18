// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc kube usage`: CPU, memory, disk and network across the cluster
//! kubectl points at.
//!
//! The nodes, the pods, the metrics-server's numbers and each kubelet's
//! stats summary are read once per look, and turned into the cluster's
//! totals, each node against what it can give, each namespace, and the
//! busiest pods. Requests are shown next to usage, so you see how much of
//! the cluster is promised away. Without metrics-server CPU and memory
//! usage is 0, and the rest is still there.
//!
//! Network counters only go up, so one look shows the bytes moved since
//! each node or pod started; [`Usage::since`] an earlier look turns them
//! into bytes a second. The kubelet refreshes them every ten seconds or
//! so, and until it does the rate from before is kept.

use crate::kube::cluster::kubectl;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

/// What one look at the cluster saw.
#[derive(Debug, Default)]
pub struct Usage {
    pub context: String,
    pub nodes: Vec<NodeUsage>,
    pub pods: Vec<PodUsage>,
    /// Whether metrics-server answered for the nodes.
    pub metrics: bool,
    /// Whether the network has rates, from [`since`](Usage::since).
    pub rates: bool,
}

/// CPU in cores, memory and disk in bytes.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct NodeUsage {
    pub name: String,
    pub cpu: f64,
    pub cpu_alloc: f64,
    pub cpu_req: f64,
    pub mem: f64,
    pub mem_alloc: f64,
    pub mem_req: f64,
    /// The kubelet's filesystem, and the ephemeral storage pods request.
    pub disk: f64,
    pub disk_cap: f64,
    pub disk_req: f64,
    pub net: Net,
    pub pods: usize,
    pub pods_alloc: u64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PodUsage {
    pub namespace: String,
    pub name: String,
    /// `-` while it isn't scheduled.
    pub node: String,
    pub cpu: f64,
    pub mem: f64,
    pub cpu_req: f64,
    pub mem_req: f64,
    /// Ephemeral storage: writable layers, logs and emptyDirs.
    pub disk: f64,
    /// What its persistent volumes hold.
    pub volumes: f64,
    pub disk_req: f64,
    /// Nothing on the host's network, that's the node's.
    pub net: Net,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct NamespaceUsage {
    pub name: String,
    pub cpu: f64,
    pub mem: f64,
    pub cpu_req: f64,
    pub mem_req: f64,
    pub disk: f64,
    pub volumes: f64,
    pub net: Net,
    pub pods: usize,
}

/// Bytes received and sent since the start, and a second once known.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Net {
    pub rx: f64,
    pub tx: f64,
    /// When the kubelet counted them, in Unix seconds.
    pub time: i64,
    pub rx_rate: Option<f64>,
    pub tx_rate: Option<f64>,
}

impl Net {
    /// The rates since `before`, or the ones from then while the counters
    /// haven't moved on. A restart, where they went down, has none.
    fn since(&mut self, before: &Net) {
        if self.time == before.time {
            (self.rx_rate, self.tx_rate) = (before.rx_rate, before.tx_rate);
        } else if self.time > before.time && self.rx >= before.rx && self.tx >= before.tx {
            let secs = (self.time - before.time) as f64;
            self.rx_rate = Some((self.rx - before.rx) / secs);
            self.tx_rate = Some((self.tx - before.tx) / secs);
        }
    }

    fn add(&mut self, other: &Net) {
        self.rx += other.rx;
        self.tx += other.tx;
        let add = |a: Option<f64>, b: Option<f64>| match (a, b) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
        };
        self.rx_rate = add(self.rx_rate, other.rx_rate);
        self.tx_rate = add(self.tx_rate, other.tx_rate);
    }

    /// What it's sorted by: the rates with `rates`, the totals otherwise.
    fn busy(&self, rates: bool) -> f64 {
        if rates {
            self.rx_rate.unwrap_or(0.0) + self.tx_rate.unwrap_or(0.0)
        } else {
            self.rx + self.tx
        }
    }

    /// (in, out) as text.
    fn show(&self, rates: bool) -> (String, String) {
        if !rates {
            return (bytes(self.rx), bytes(self.tx));
        }
        let rate = |r: Option<f64>| r.map_or_else(|| "-".into(), |r| format!("{}/s", bytes(r)));
        (rate(self.rx_rate), rate(self.tx_rate))
    }
}

impl Usage {
    /// Reads the cluster behind `context`, only the pods of `namespaces`
    /// when there are any. The reads run side by side, each node's stats
    /// summary once the nodes are known.
    pub fn read(context: &str, namespaces: &[String]) -> Result<Usage> {
        let get = |args: &[&str]| {
            let mut all = vec!["--context", context];
            all.extend(args);
            kubectl(&all)
        };
        let join = |h: std::thread::ScopedJoinHandle<'_, Result<String>>| {
            h.join().expect("kubectl reader panicked")
        };
        let (nodes, pods, node_metrics, pod_metrics, summaries) = std::thread::scope(|s| {
            let pods = s.spawn(|| get(&["get", "pods", "-A", "-o", "json"]));
            let nm = s.spawn(|| get(&["get", "--raw", "/apis/metrics.k8s.io/v1beta1/nodes"]));
            let pm = s.spawn(|| get(&["get", "--raw", "/apis/metrics.k8s.io/v1beta1/pods"]));
            let nodes = get(&["get", "nodes", "-o", "json"]);
            let names: Vec<String> = nodes
                .as_deref()
                .ok()
                .and_then(|n| items::<Node>(n).ok())
                .unwrap_or_default()
                .into_iter()
                .map(|n| n.metadata.name)
                .collect();
            let summaries: Vec<_> = names
                .iter()
                .map(|n| {
                    let path = format!("/api/v1/nodes/{n}/proxy/stats/summary");
                    s.spawn(move || get(&["get", "--raw", &path]))
                })
                .collect();
            // A node that's down has no summary, and shows 0.
            let summaries: Vec<String> =
                summaries.into_iter().filter_map(|h| join(h).ok()).collect();
            (nodes, join(pods), join(nm), join(pm), summaries)
        });
        // No metrics-server is fine: the usage is 0 then.
        let empty = || r#"{"items":[]}"#.to_string();
        Usage::parse(
            context,
            &nodes?,
            &pods?,
            &node_metrics.unwrap_or_else(|_| empty()),
            &pod_metrics.unwrap_or_else(|_| empty()),
            &summaries,
            namespaces,
        )
    }

    /// Puts together what kubectl returned, as [`read`](Usage::read) does.
    pub fn parse(
        context: &str,
        nodes: &str,
        pods: &str,
        node_metrics: &str,
        pod_metrics: &str,
        summaries: &[String],
        namespaces: &[String],
    ) -> Result<Usage> {
        let summaries: Vec<Summary> = summaries
            .iter()
            .map(|s| serde_json::from_str(s))
            .collect::<Result<_, _>>()
            .context("reading the kubelet stats")?;
        let node_stats: HashMap<&str, &NodeStats> = summaries
            .iter()
            .map(|s| (s.node.node_name.as_str(), &s.node))
            .collect();
        let pod_stats: HashMap<(&str, &str), &PodStats> = summaries
            .iter()
            .flat_map(|s| &s.pods)
            .map(|p| ((p.pod_ref.namespace.as_str(), p.pod_ref.name.as_str()), p))
            .collect();
        let node_metrics: Vec<Metrics> = items(node_metrics).context("reading node metrics")?;
        let pod_metrics: Vec<Metrics> = items(pod_metrics).context("reading pod metrics")?;
        let node_usage: HashMap<&str, (f64, f64)> = node_metrics
            .iter()
            .map(|m| (m.metadata.name.as_str(), m.total()))
            .collect();
        let pod_usage: HashMap<(&str, &str), (f64, f64)> = pod_metrics
            .iter()
            .map(|m| {
                (
                    (m.metadata.namespace.as_str(), m.metadata.name.as_str()),
                    m.total(),
                )
            })
            .collect();

        let mut pods: Vec<PodUsage> = items::<Pod>(pods)
            .context("reading the pods")?
            .into_iter()
            .filter(|p| namespaces.is_empty() || namespaces.contains(&p.metadata.namespace))
            .map(|p| {
                let (cpu, mem) = pod_usage
                    .get(&(p.metadata.namespace.as_str(), p.metadata.name.as_str()))
                    .copied()
                    .unwrap_or_default();
                // A finished pod holds on to nothing.
                let done = matches!(p.status.phase.as_str(), "Succeeded" | "Failed");
                let request = |name: &str| -> f64 {
                    if done {
                        return 0.0;
                    }
                    let containers = p.spec.containers.iter();
                    containers
                        .map(|c| c.resources.requests.get(name).map_or(0.0, |q| quantity(q)))
                        .sum()
                };
                let stats =
                    pod_stats.get(&(p.metadata.namespace.as_str(), p.metadata.name.as_str()));
                let (disk, volumes, net) = stats.map_or_else(Default::default, |s| {
                    let volumes = s.volume.iter().filter(|v| v.pvc_ref.is_some());
                    let net = if p.spec.host_network {
                        Net::default()
                    } else {
                        s.network.net()
                    };
                    (
                        s.ephemeral.used_bytes,
                        volumes.map(|v| v.used_bytes).sum(),
                        net,
                    )
                });
                PodUsage {
                    cpu_req: request("cpu"),
                    mem_req: request("memory"),
                    disk_req: request("ephemeral-storage"),
                    disk,
                    volumes,
                    net,
                    node: p.spec.node_name.clone().unwrap_or_else(|| "-".into()),
                    namespace: p.metadata.namespace,
                    name: p.metadata.name,
                    cpu,
                    mem,
                }
            })
            .collect();
        pods.sort_by(|a, b| (&a.namespace, &a.name).cmp(&(&b.namespace, &b.name)));

        let mut nodes: Vec<NodeUsage> = items::<Node>(nodes)
            .context("reading the nodes")?
            .into_iter()
            .map(|n| {
                let alloc =
                    |name: &str| n.status.allocatable.get(name).map_or(0.0, |q| quantity(q));
                let (cpu, mem) = node_usage
                    .get(n.metadata.name.as_str())
                    .copied()
                    .unwrap_or_default();
                let on: Vec<&PodUsage> =
                    pods.iter().filter(|p| p.node == n.metadata.name).collect();
                let stats = node_stats.get(n.metadata.name.as_str());
                NodeUsage {
                    disk: stats.map_or(0.0, |s| s.fs.used_bytes),
                    disk_cap: stats.map_or(0.0, |s| s.fs.capacity_bytes),
                    disk_req: on.iter().map(|p| p.disk_req).sum(),
                    net: stats.map(|s| s.network.net()).unwrap_or_default(),
                    cpu,
                    cpu_alloc: alloc("cpu"),
                    cpu_req: on.iter().map(|p| p.cpu_req).sum(),
                    mem,
                    mem_alloc: alloc("memory"),
                    mem_req: on.iter().map(|p| p.mem_req).sum(),
                    pods: on.len(),
                    pods_alloc: alloc("pods") as u64,
                    name: n.metadata.name,
                }
            })
            .collect();
        nodes.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(Usage {
            context: context.to_string(),
            metrics: !node_metrics.is_empty(),
            rates: false,
            nodes,
            pods,
        })
    }

    /// Turns the network counters into bytes a second, from what they
    /// were in `before`, an earlier look at the same cluster.
    pub fn since(&mut self, before: &Usage) {
        let nodes: HashMap<&str, &Net> = before
            .nodes
            .iter()
            .map(|n| (n.name.as_str(), &n.net))
            .collect();
        for n in &mut self.nodes {
            if let Some(b) = nodes.get(n.name.as_str()) {
                n.net.since(b);
            }
        }
        let pods: HashMap<(&str, &str), &Net> = before
            .pods
            .iter()
            .map(|p| ((p.namespace.as_str(), p.name.as_str()), &p.net))
            .collect();
        for p in &mut self.pods {
            if let Some(b) = pods.get(&(p.namespace.as_str(), p.name.as_str())) {
                p.net.since(b);
            }
        }
        self.rates = true;
    }

    /// Each namespace's pods added up, by name.
    pub fn namespaces(&self) -> Vec<NamespaceUsage> {
        let mut by: BTreeMap<&str, NamespaceUsage> = BTreeMap::new();
        for p in &self.pods {
            let ns = by.entry(&p.namespace).or_insert_with(|| NamespaceUsage {
                name: p.namespace.clone(),
                ..NamespaceUsage::default()
            });
            ns.cpu += p.cpu;
            ns.mem += p.mem;
            ns.cpu_req += p.cpu_req;
            ns.mem_req += p.mem_req;
            ns.disk += p.disk;
            ns.volumes += p.volumes;
            ns.net.add(&p.net);
            ns.pods += 1;
        }
        by.into_values().collect()
    }

    /// The `n` pods using the most of what `key` picks.
    pub fn top(&self, n: usize, key: impl Fn(&PodUsage) -> f64) -> Vec<&PodUsage> {
        let mut pods: Vec<&PodUsage> = self.pods.iter().collect();
        pods.sort_by(|a, b| key(b).total_cmp(&key(a)));
        pods.truncate(n);
        pods
    }

    /// The report, with ANSI colours when `color`.
    pub fn render(&self, top: usize, color: bool) -> String {
        let p = Paint(color);
        let mut out = String::new();
        let o = &mut out;
        let _ = writeln!(
            o,
            "{}  {}",
            p.bold(&self.context),
            p.dim(&format!(
                "{} nodes · {} pods",
                self.nodes.len(),
                self.pods.len()
            ))
        );
        if !self.metrics {
            let _ = writeln!(
                o,
                "{}",
                p.paint(YELLOW, "no metrics-server data, so usage shows 0")
            );
        }
        let _ = writeln!(o);

        let sum = |f: fn(&NodeUsage) -> f64| self.nodes.iter().map(f).sum::<f64>();
        let cpu_req: f64 = self.pods.iter().map(|p| p.cpu_req).sum();
        let mem_req: f64 = self.pods.iter().map(|p| p.mem_req).sum();
        let disk_req: f64 = self.pods.iter().map(|p| p.disk_req).sum();
        let total =
            |o: &mut String, what: &str, used: String, of: String, pct: f64, req: String| {
                let _ = writeln!(
                    o,
                    "  {what:<4}  {} of {of}  {}  [{}]  {}",
                    p.paint(CYAN, &used),
                    p.pct(pct, &format!("{pct:5.1}%")),
                    p.pct(pct, &bar(pct, 24)),
                    p.dim(&format!("requests {req}")),
                );
            };
        let _ = writeln!(o, "{}", p.bold("CLUSTER"));
        let (cpu, alloc) = (sum(|n| n.cpu), sum(|n| n.cpu_alloc));
        total(
            o,
            "CPU",
            cpu_str(cpu),
            format!("{} cores", cpu_str(alloc)),
            pct(cpu, alloc),
            format!("{} cores", cpu_str(cpu_req)),
        );
        let (mem, alloc) = (sum(|n| n.mem), sum(|n| n.mem_alloc));
        total(
            o,
            "MEM",
            bytes(mem),
            bytes(alloc),
            pct(mem, alloc),
            bytes(mem_req),
        );
        let (disk, cap) = (sum(|n| n.disk), sum(|n| n.disk_cap));
        total(
            o,
            "DISK",
            bytes(disk),
            bytes(cap),
            pct(disk, cap),
            bytes(disk_req),
        );
        let mut net = Net::default();
        self.nodes.iter().for_each(|n| net.add(&n.net));
        let (rx, tx) = net.show(self.rates);
        let _ = writeln!(
            o,
            "  NET   in {}  out {}  {}",
            p.paint(CYAN, &rx),
            p.paint(CYAN, &tx),
            p.dim(if self.rates {
                "on the nodes' own interface"
            } else {
                "since each node started, -w for rates"
            }),
        );
        let _ = writeln!(o);

        let _ = writeln!(o, "{}", p.bold("NODES"));
        let _ = writeln!(
            o,
            "{}",
            p.dim(&format!(
                "{:<16} {:>6} {:>6} {:>6} {:>6}  {:>10} {:>10} {:>6} {:>10}  {:>8}",
                "NODE", "CPU", "OF", "%", "REQ", "MEM", "OF", "%", "REQ", "PODS"
            ))
        );
        for n in &self.nodes {
            let (cpc, mpc) = (pct(n.cpu, n.cpu_alloc), pct(n.mem, n.mem_alloc));
            let _ = writeln!(
                o,
                "{:<16} {:>6} {:>6} {} {:>6}  {:>10} {:>10} {} {:>10}  {:>8}",
                n.name,
                cpu_str(n.cpu),
                cpu_str(n.cpu_alloc),
                p.pct(cpc, &format!("{cpc:>5.1}%")),
                cpu_str(n.cpu_req),
                bytes(n.mem),
                bytes(n.mem_alloc),
                p.pct(mpc, &format!("{mpc:>5.1}%")),
                bytes(n.mem_req),
                format!("{}/{}", n.pods, n.pods_alloc),
            );
        }
        let _ = writeln!(o);

        let (rx, tx) = if self.rates {
            ("IN/s", "OUT/s")
        } else {
            ("IN", "OUT")
        };
        let _ = writeln!(o, "{}", p.bold("NODE DISK AND NETWORK"));
        let _ = writeln!(
            o,
            "{}",
            p.dim(&format!(
                "{:<16} {:>10} {:>10} {:>6} {:>10}  {:>12} {:>12}",
                "NODE", "DISK", "OF", "%", "REQ", rx, tx
            ))
        );
        for n in &self.nodes {
            let dpc = pct(n.disk, n.disk_cap);
            let (nrx, ntx) = n.net.show(self.rates);
            let _ = writeln!(
                o,
                "{:<16} {:>10} {:>10} {} {:>10}  {:>12} {:>12}",
                n.name,
                bytes(n.disk),
                bytes(n.disk_cap),
                p.pct(dpc, &format!("{dpc:>5.1}%")),
                bytes(n.disk_req),
                nrx,
                ntx,
            );
        }
        let _ = writeln!(o);

        let namespaces = self.namespaces();
        if !namespaces.is_empty() {
            let _ = writeln!(o, "{}", p.bold("NAMESPACES"));
            let _ = writeln!(
                o,
                "{}",
                p.dim(&format!(
                    "{:<20} {:>6} {:>6}  {:>10} {:>10}  {:>10} {:>10}  {:>12} {:>12}  {:>5}",
                    "NAMESPACE", "CPU", "REQ", "MEM", "REQ", "DISK", "VOLUMES", rx, tx, "PODS"
                ))
            );
            for ns in &namespaces {
                let (nrx, ntx) = ns.net.show(self.rates);
                let _ = writeln!(
                    o,
                    "{:<20} {:>6} {:>6}  {:>10} {:>10}  {:>10} {:>10}  {:>12} {:>12}  {:>5}",
                    ns.name,
                    cpu_str(ns.cpu),
                    cpu_str(ns.cpu_req),
                    bytes(ns.mem),
                    bytes(ns.mem_req),
                    bytes(ns.disk),
                    bytes(ns.volumes),
                    nrx,
                    ntx,
                    ns.pods
                );
            }
            let _ = writeln!(o);
        }

        // A top-pod table: its title, the pods, the columns' names and
        // each pod's values in them.
        let pods = |o: &mut String,
                    title: String,
                    pods: Vec<&PodUsage>,
                    head: &[&str],
                    row: &dyn Fn(&PodUsage) -> Vec<String>| {
            let _ = writeln!(o, "{}", p.bold(&title));
            if pods.is_empty() {
                let _ = writeln!(o, "{}\n", p.dim("  (nothing)"));
                return;
            }
            let line = |cells: &[String]| {
                let mut line = format!("{:<20} {:<34} {:<16}", cells[0], cells[1], cells[2]);
                for c in &cells[3..] {
                    let _ = write!(line, " {c:>12}");
                }
                line
            };
            let head: Vec<String> = ["NAMESPACE", "POD", "NODE"]
                .iter()
                .chain(head)
                .map(|s| s.to_string())
                .collect();
            let _ = writeln!(o, "{}", p.dim(&line(&head)));
            for pod in pods {
                let mut cells = vec![
                    pod.namespace.clone(),
                    shorten(&pod.name, 34),
                    pod.node.clone(),
                ];
                cells.extend(row(pod));
                let _ = writeln!(o, "{}", line(&cells));
            }
            let _ = writeln!(o);
        };
        let compute = |pod: &PodUsage| {
            vec![
                cpu_str(pod.cpu),
                bytes(pod.mem),
                cpu_str(pod.cpu_req),
                bytes(pod.mem_req),
            ]
        };
        let compute_head = ["CPU", "MEM", "CPU REQ", "MEM REQ"];
        pods(
            o,
            format!("TOP {top} PODS BY CPU"),
            self.top(top, |p| p.cpu),
            &compute_head,
            &compute,
        );
        pods(
            o,
            format!("TOP {top} PODS BY MEMORY"),
            self.top(top, |p| p.mem),
            &compute_head,
            &compute,
        );
        pods(
            o,
            format!("TOP {top} PODS BY DISK"),
            self.top(top, |p| p.disk + p.volumes),
            &["DISK", "VOLUMES", "REQ"],
            &|pod| vec![bytes(pod.disk), bytes(pod.volumes), bytes(pod.disk_req)],
        );
        pods(
            o,
            format!("TOP {top} PODS BY NETWORK"),
            self.top(top, |p| p.net.busy(self.rates)),
            &[rx, tx],
            &|pod| {
                let (rx, tx) = pod.net.show(self.rates);
                vec![rx, tx]
            },
        );
        out
    }
}

/// A Kubernetes quantity (`250m`, `1.5`, `512Mi`, `2G`, `1e3`) as a plain
/// number: cores for CPU, bytes for memory. Anything unreadable is 0.
pub fn quantity(q: &str) -> f64 {
    const SUFFIXES: [(&str, f64); 15] = [
        ("Ki", 1024.0),
        ("Mi", 1048576.0),
        ("Gi", 1073741824.0),
        ("Ti", 1099511627776.0),
        ("Pi", 1125899906842624.0),
        ("Ei", 1152921504606846976.0),
        ("n", 1e-9),
        ("u", 1e-6),
        ("m", 1e-3),
        ("k", 1e3),
        ("M", 1e6),
        ("G", 1e9),
        ("T", 1e12),
        ("P", 1e15),
        ("E", 1e18),
    ];
    let q = q.trim();
    let (number, scale) = SUFFIXES
        .iter()
        .find_map(|(suffix, scale)| q.strip_suffix(suffix).map(|n| (n, *scale)))
        .unwrap_or((q, 1.0));
    number.parse::<f64>().map_or(0.0, |n| n * scale)
}

/// Cores, in millicores below a tenth of one.
fn cpu_str(cores: f64) -> String {
    if cores > 0.0 && cores < 0.1 {
        format!("{:.0}m", cores * 1000.0)
    } else {
        format!("{cores:.2}")
    }
}

fn bytes(b: f64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let (mut b, mut unit) = (b, 0);
    while b >= 1024.0 && unit < UNITS.len() - 1 {
        b /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{b:.0} B")
    } else {
        format!("{b:.1} {}", UNITS[unit])
    }
}

fn pct(part: f64, whole: f64) -> f64 {
    if whole > 0.0 {
        100.0 * part / whole
    } else {
        0.0
    }
}

fn bar(pct: f64, width: usize) -> String {
    let filled = ((pct.clamp(0.0, 100.0) * width as f64 / 100.0).round() as usize).min(width);
    format!("{}{}", "#".repeat(filled), ".".repeat(width - filled))
}

fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max - 3).collect();
    format!("{head}...")
}

const GREEN: &str = "32";
const YELLOW: &str = "33";
const RED: &str = "31";
const CYAN: &str = "36";

/// ANSI colours, or nothing at all.
struct Paint(bool);

impl Paint {
    fn paint(&self, code: &str, s: &str) -> String {
        if self.0 {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_string()
        }
    }
    fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
    fn dim(&self, s: &str) -> String {
        self.paint("2", s)
    }
    /// Green under 70%, yellow under 90%, red from there.
    fn pct(&self, pct: f64, s: &str) -> String {
        let code = if pct >= 90.0 {
            RED
        } else if pct >= 70.0 {
            YELLOW
        } else {
            GREEN
        };
        self.paint(code, s)
    }
}

#[derive(Deserialize)]
struct List<T> {
    #[serde(default = "Vec::new")]
    items: Vec<T>,
}

fn items<T: DeserializeOwned>(json: &str) -> Result<Vec<T>> {
    Ok(serde_json::from_str::<List<T>>(json)?.items)
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Meta {
    name: String,
    namespace: String,
}

type Quantities = BTreeMap<String, String>;

#[derive(Deserialize, Default)]
#[serde(default)]
struct Node {
    metadata: Meta,
    status: NodeStatus,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct NodeStatus {
    allocatable: Quantities,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Pod {
    metadata: Meta,
    spec: PodSpec,
    status: PodStatus,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct PodSpec {
    node_name: Option<String>,
    host_network: bool,
    containers: Vec<Container>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct PodStatus {
    phase: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Container {
    resources: Resources,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Resources {
    requests: Quantities,
}

/// A kubelet's `/stats/summary`: its node, and the pods on it.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Summary {
    node: NodeStats,
    pods: Vec<PodStats>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct NodeStats {
    node_name: String,
    fs: FsStats,
    network: NetStats,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct PodStats {
    pod_ref: Meta,
    #[serde(rename = "ephemeral-storage")]
    ephemeral: FsStats,
    volume: Vec<VolumeStats>,
    network: NetStats,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct FsStats {
    capacity_bytes: f64,
    used_bytes: f64,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct VolumeStats {
    pvc_ref: Option<Meta>,
    used_bytes: f64,
}

/// The default interface's counters.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct NetStats {
    time: String,
    rx_bytes: f64,
    tx_bytes: f64,
}

impl NetStats {
    fn net(&self) -> Net {
        Net {
            rx: self.rx_bytes,
            tx: self.tx_bytes,
            time: unix(&self.time).unwrap_or(0),
            ..Net::default()
        }
    }
}

/// `2026-10-18T12:03:04Z` in Unix seconds.
fn unix(t: &str) -> Option<i64> {
    let n = |at: std::ops::Range<usize>| t.get(at)?.parse::<i64>().ok();
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let secs = n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)?;
    // Days since 1970 of a proleptic Gregorian date, years starting in March.
    let y = if m <= 2 { y - 1 } else { y };
    let (era, yoe) = (y.div_euclid(400), y.rem_euclid(400));
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + secs)
}

/// A NodeMetrics or PodMetrics of metrics.k8s.io.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Metrics {
    metadata: Meta,
    usage: Quantities,
    containers: Vec<ContainerMetrics>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ContainerMetrics {
    usage: Quantities,
}

impl Metrics {
    /// (cores, bytes): a node's own usage, or its containers' added up.
    fn total(&self) -> (f64, f64) {
        let get = |usage: &Quantities, name: &str| usage.get(name).map_or(0.0, |q| quantity(q));
        let of = |name: &str| match self.usage.get(name) {
            Some(q) => quantity(q),
            None => self.containers.iter().map(|c| get(&c.usage, name)).sum(),
        };
        (of("cpu"), of("memory"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantities() {
        assert_eq!(quantity("250m"), 0.25);
        assert_eq!(quantity("2"), 2.0);
        assert_eq!(quantity("1.5"), 1.5);
        assert!((quantity("123456789n") - 0.123456789).abs() < 1e-12);
        assert_eq!(quantity("512Mi"), 512.0 * 1048576.0);
        assert_eq!(quantity("16318848Ki"), 16318848.0 * 1024.0);
        assert_eq!(quantity("2G"), 2e9);
        assert_eq!(quantity("1e3"), 1000.0);
        assert_eq!(quantity("110"), 110.0);
        assert_eq!(quantity("nonsense"), 0.0);
    }

    #[test]
    fn formats() {
        assert_eq!(cpu_str(0.05), "50m");
        assert_eq!(cpu_str(0.0), "0.00");
        assert_eq!(cpu_str(1.234), "1.23");
        assert_eq!(bytes(512.0), "512 B");
        assert_eq!(bytes(1536.0 * 1048576.0), "1.5 GiB");
        assert_eq!(bar(50.0, 10), "#####.....");
        assert_eq!(bar(150.0, 4), "####");
        assert_eq!(shorten("abcdefgh", 6), "abc...");
    }

    const NODES: &str = r#"{"items":[
        {"metadata":{"name":"w-1"},"status":{"allocatable":{"cpu":"4","memory":"8Gi","pods":"110"}}},
        {"metadata":{"name":"cp-1"},"status":{"allocatable":{"cpu":"2","memory":"4Gi","pods":"110"}}}
    ]}"#;
    const PODS: &str = r#"{"items":[
        {"metadata":{"name":"web-1","namespace":"shop"},
         "spec":{"nodeName":"w-1","containers":[
            {"resources":{"requests":{"cpu":"500m","memory":"256Mi"}}},
            {"resources":{}}]},
         "status":{"phase":"Running"}},
        {"metadata":{"name":"migrate-x","namespace":"shop"},
         "spec":{"nodeName":"w-1","containers":[{"resources":{"requests":{"cpu":"1"}}}]},
         "status":{"phase":"Succeeded"}},
        {"metadata":{"name":"coredns","namespace":"kube-system"},
         "spec":{"nodeName":"cp-1","containers":[{"resources":{"requests":{"cpu":"100m","memory":"70Mi"}}}]},
         "status":{"phase":"Running"}},
        {"metadata":{"name":"pending","namespace":"shop"},
         "spec":{"containers":[{"resources":{"requests":{"cpu":"2"}}}]},
         "status":{"phase":"Pending"}}
    ]}"#;
    const NODE_METRICS: &str = r#"{"items":[
        {"metadata":{"name":"w-1"},"usage":{"cpu":"1500000000n","memory":"2Gi"}},
        {"metadata":{"name":"cp-1"},"usage":{"cpu":"250m","memory":"1Gi"}}
    ]}"#;
    const POD_METRICS: &str = r#"{"items":[
        {"metadata":{"name":"web-1","namespace":"shop"},"containers":[
            {"usage":{"cpu":"300m","memory":"100Mi"}},{"usage":{"cpu":"200m","memory":"28Mi"}}]},
        {"metadata":{"name":"coredns","namespace":"kube-system"},"containers":[
            {"usage":{"cpu":"5m","memory":"20Mi"}}]}
    ]}"#;

    const W1_SUMMARY: &str = r#"{
        "node":{"nodeName":"w-1",
            "fs":{"capacityBytes":40000000000,"usedBytes":10000000000},
            "network":{"time":"2026-10-18T12:00:00Z","rxBytes":1000,"txBytes":2000}},
        "pods":[
            {"podRef":{"name":"web-1","namespace":"shop"},
             "ephemeral-storage":{"usedBytes":4096},
             "volume":[{"name":"tmp","usedBytes":100},
                       {"name":"data","pvcRef":{"name":"data-web-1","namespace":"shop"},"usedBytes":5000}],
             "network":{"time":"2026-10-18T12:00:00Z","rxBytes":300,"txBytes":400}}
        ]}"#;
    const CP1_SUMMARY: &str = r#"{
        "node":{"nodeName":"cp-1",
            "fs":{"capacityBytes":20000000000,"usedBytes":5000000000},
            "network":{"time":"2026-10-18T12:00:00Z","rxBytes":10,"txBytes":20}},
        "pods":[
            {"podRef":{"name":"coredns","namespace":"kube-system"},
             "ephemeral-storage":{"usedBytes":8192},
             "network":{"time":"2026-10-18T12:00:00Z","rxBytes":50,"txBytes":60}}
        ]}"#;

    fn usage(namespaces: &[&str]) -> Usage {
        let ns: Vec<String> = namespaces.iter().map(|s| s.to_string()).collect();
        let summaries = [W1_SUMMARY.to_string(), CP1_SUMMARY.to_string()];
        Usage::parse(
            "test",
            NODES,
            PODS,
            NODE_METRICS,
            POD_METRICS,
            &summaries,
            &ns,
        )
        .unwrap()
    }

    #[test]
    fn nodes_add_up_their_pods() {
        let u = usage(&[]);
        assert!(u.metrics);
        let names: Vec<&str> = u.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, ["cp-1", "w-1"]);
        let w = &u.nodes[1];
        assert_eq!(w.cpu, 1.5);
        assert_eq!(w.cpu_alloc, 4.0);
        // The finished migration holds nothing.
        assert_eq!(w.cpu_req, 0.5);
        assert_eq!(w.mem_req, 256.0 * 1048576.0);
        assert_eq!(w.pods, 2);
        assert_eq!(w.pods_alloc, 110);
    }

    #[test]
    fn pods_add_up_their_containers() {
        let u = usage(&[]);
        let web = u.pods.iter().find(|p| p.name == "web-1").unwrap();
        assert_eq!(web.cpu, 0.5);
        assert_eq!(web.mem, 128.0 * 1048576.0);
        let pending = u.pods.iter().find(|p| p.name == "pending").unwrap();
        assert_eq!(pending.node, "-");
        assert_eq!(u.top(1, |p| p.cpu)[0].name, "web-1");
    }

    #[test]
    fn namespaces_by_name() {
        let ns = usage(&[]).namespaces();
        assert_eq!(ns.len(), 2);
        assert_eq!(ns[0].name, "kube-system");
        assert_eq!(ns[1].name, "shop");
        assert_eq!(ns[1].pods, 3);
        assert_eq!(ns[1].cpu_req, 2.5);
    }

    #[test]
    fn only_the_namespaces_asked_for() {
        let u = usage(&["kube-system"]);
        assert_eq!(u.pods.len(), 1);
        assert_eq!(u.nodes[1].pods, 0);
    }

    #[test]
    fn without_metrics_server() {
        let empty = r#"{"items":[]}"#;
        let u = Usage::parse("test", NODES, PODS, empty, empty, &[], &[]).unwrap();
        assert!(!u.metrics);
        assert_eq!(u.nodes[0].cpu, 0.0);
        assert!(u.render(5, false).contains("no metrics-server data"));
    }

    #[test]
    fn renders_plain() {
        let out = usage(&[]).render(5, false);
        assert!(!out.contains('\x1b'));
        assert!(out.contains("TOP 5 PODS BY CPU"));
        assert!(out.contains("CPU   1.75 of 6.00 cores"));
        assert!(out.contains("DISK  14.0 GiB of 55.9 GiB"));
        assert!(out.contains("since each node started"));
        assert!(out.contains("TOP 5 PODS BY NETWORK"));
    }

    #[test]
    fn disk_from_the_kubelet() {
        let u = usage(&[]);
        let w = &u.nodes[1];
        assert_eq!(w.disk, 1e10);
        assert_eq!(w.disk_cap, 4e10);
        let web = u.pods.iter().find(|p| p.name == "web-1").unwrap();
        // The emptyDir is in the ephemeral storage already.
        assert_eq!(web.disk, 4096.0);
        assert_eq!(web.volumes, 5000.0);
        let ns = u.namespaces();
        assert_eq!(ns[1].volumes, 5000.0);
    }

    #[test]
    fn rates_since_an_earlier_look() {
        let before = usage(&[]);
        let mut u = usage(&[]);
        u.since(&before);
        // The kubelet hasn't counted again yet: no rate so far.
        assert!(u.rates);
        assert_eq!(u.nodes[1].net.rx_rate, None);

        let mut later = usage(&[]);
        later.nodes[1].net = Net {
            rx: 11000.0,
            tx: 2000.0,
            time: before.nodes[1].net.time + 10,
            ..Net::default()
        };
        later.since(&u);
        assert_eq!(later.nodes[1].net.rx_rate, Some(1000.0));
        assert_eq!(later.nodes[1].net.tx_rate, Some(0.0));

        // Counted the same as before: the rate from then stays.
        let mut again = usage(&[]);
        again.nodes[1].net = later.nodes[1].net.clone();
        again.nodes[1].net.rx_rate = None;
        again.since(&later);
        assert_eq!(again.nodes[1].net.rx_rate, Some(1000.0));
        assert!(again.render(5, false).contains("IN/s"));
    }

    #[test]
    fn host_network_pods_have_none() {
        let pods = r#"{"items":[
            {"metadata":{"name":"coredns","namespace":"kube-system"},
             "spec":{"nodeName":"cp-1","hostNetwork":true,"containers":[]},
             "status":{"phase":"Running"}}]}"#;
        let empty = r#"{"items":[]}"#;
        let u = Usage::parse(
            "test",
            NODES,
            pods,
            empty,
            empty,
            &[CP1_SUMMARY.into()],
            &[],
        )
        .unwrap();
        assert_eq!(u.pods[0].net, Net::default());
        assert_eq!(u.pods[0].disk, 8192.0);
    }

    #[test]
    fn timestamps() {
        assert_eq!(unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(unix("2000-03-01T00:00:01Z"), Some(951868801));
        assert_eq!(unix("2026-10-18T12:03:04Z"), Some(1792324984));
        assert_eq!(unix(""), None);
    }
}
