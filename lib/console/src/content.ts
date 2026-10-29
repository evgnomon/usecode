// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The site's copy, as plain data, so the views stay about layout.

export const REPO_URL = "https://github.com/evgnomon/usecode";
export const CONTACT = "mailto:hg@evgnomon.org";

// What the hosted plan costs; the Stripe price behind it is STRIPE_PRICE_ID.
export const HOSTED_PRICE = {
  amount: "$10",
  period: "per user a month",
};

// The line the paid plans are sold on.
export const PROMISE = "We make sure it doesn't break.";

export const PLANS = {
  individual: {
    name: "Individual",
    price: "Free",
    note: "All of usecode, on your own nodes, run by you and your agents.",
    points: [
      "Every feature, open source",
      "Unlimited nodes, edge or Kubernetes",
      "A private WireGuard mesh between your nodes",
      "API access for your agents",
    ],
  },
  hosted: {
    name: "Hosted",
    price: HOSTED_PRICE.amount,
    note: "The same usecode, with the heavy parts run by us, outside your cluster.",
    points: [
      "Monitoring and alerts, watched from outside your nodes",
      "An audit log of what changed, kept where no one can rewrite it",
      "Backups stored off your nodes",
      "Upgrades and uptime of what we host are on us",
    ],
  },
  enterprise: {
    name: "Enterprise",
    price: "Let's talk",
    note: PROMISE,
    points: [
      "Everything in Hosted",
      "We review what your agents plan to change in production",
      "We keep it working through upgrades and changes upstream",
      "Someone who knows your setup answers when it breaks",
      "A contract, invoicing and terms that fit your company",
    ],
  },
};

/**
 * A journey reads "From here to there", then what you don't have to do. The
 * prompt is what you'd ask the agent, the steps what it does about it.
 */
export type Journey = { from: string; without: string; prompt: string; steps: string[] };

// The opening of the landing page. The hero rolls the journeys three at a
// time, and plays the one in the middle in a console underneath.
export const MOTIVATION = {
  tagline: "Use code until you see it yours!",
  journeys: [
    {
      from: "From one agent to a team,",
      without: "without relearning what’s known.",
      prompt: "give the new agent what we know",
      steps: ["Shared the workspace and its notes", "Same nodes, same keys, scoped", "Second agent picked up the queue", "2 agents, 0 repeated work"],
    },
    {
      from: "From your first user to your billionth,",
      without: "without starting over.",
      prompt: "we're on the front page, hold on",
      steps: ["Traffic up 40x in 6 minutes", "Joined 4 nodes to the mesh", "Database read replicas added", "Holding, errors at 0.01%"],
    },
    {
      from: "From our servers to yours,",
      without: "without lock-in.",
      prompt: "move everything onto my own servers",
      steps: ["Your 3 servers joined the mesh", "Copied the data, checksums match", "DNS switched over", "Off ours, on yours"],
    },
    {
      from: "From one cloud to the next,",
      without: "without rewriting a thing.",
      prompt: "it's cheaper over there, move us",
      steps: ["New nodes up on the other cloud", "Same config, applied as is", "Traffic moved, no errors", "Old nodes drained and gone"],
    },
    {
      from: "From someone else’s cloud to your own nodes,",
      without: "without handing over your data.",
      prompt: "keep the data on our own hardware",
      steps: ["The rack in the office joined the mesh", "Storage pinned to your nodes", "Encrypted between every host", "Your data, on your disks"],
    },
    {
      from: "From a fresh host to a working node,",
      without: "without a playbook.",
      prompt: "here's a new box, set it up",
      steps: ["usecoded installed", "Joined the WireGuard mesh", "kubelet up, node ready", "node-05 settled"],
    },
    {
      from: "From a vulnerable kernel to a patched one,",
      without: "without downtime.",
      prompt: "there's a kernel CVE out, patch us",
      steps: ["3 of 5 nodes affected", "Draining node-02, pods moved", "Patched and rebooted, one at a time", "All 5 nodes patched, 0 requests dropped"],
    },
    {
      from: "From git push to running,",
      without: "without a rented pipeline.",
      prompt: "deploy every push to main",
      steps: ["Watching main on GitHub", "a1f3c9e pushed, building", "Tests green, image pushed", "Rolled out in 48s"],
    },
    {
      from: "From “it’s down?” to “here’s why”,",
      without: "built in.",
      prompt: "checkout is failing, why?",
      steps: ["Read logs across 3 nodes", "500s since 14:02, a bad migration", "Rolled back to v41", "Checkout working, fix in a branch"],
    },
    {
      from: "From a domain name to email and DNS,",
      without: "without five SaaS bills.",
      prompt: "set up mail and DNS for shop.dev",
      steps: ["Zone created, records applied", "Mail server up, DKIM signed", "SPF and DMARC in place", "hello@shop.dev receiving"],
    },
    {
      from: "From any agent to the same machines,",
      without: "without a new integration.",
      prompt: "let my other agent use these nodes",
      steps: ["Issued it an API key", "Same MCP tools, same cluster", "It deployed its first service", "2 agents, one place to run"],
    },
    {
      from: "From a hosted model to your own,",
      without: "without changing the workflow.",
      prompt: "run our own model on the GPU box",
      steps: ["GPU node found, drivers ready", "Model pulled, 40 GB", "Serving on the same API", "Your agents now use it"],
    },
    {
      from: "From side project to paying customers,",
      without: "without building billing.",
      prompt: "let people pay for this",
      steps: ["Plans and prices set up", "Checkout page added", "Receipts and invoices on", "First customer paid"],
    },
  ] as Journey[],
  close: "From burning tokens to compounding them.",
  kicker: "Not just another coding agent.",
  body:
    "The agent writes the code, tests it, builds it, deploys it, and patches " +
    "it when something breaks. But code is the easy half: it also needs " +
    "machines, networks, databases, DNS, mail and secrets, and no coding " +
    "agent comes with those. usecode does. It isn't another coding agent; " +
    "it's everything you need a coding agent for, and the toolbox to run it " +
    "on any node, anywhere, configured the way you need it or the way that " +
    "works best.",
  team:
    "Every tool for that job is already in uc, one command line that you, " +
    "your team and your agents share. What one of you works out, the next " +
    "one simply runs. And when a tool is missing, the agent adds it, so the " +
    "next job starts further ahead.",
  ship: "Not just another coding agent. The whole toolbox, and the agent to add even more.",
};

/** A few of uc's commands, shown in the hero's terminal frame. */
export const TOOLS = [
  { cmd: "uc vm", does: "machines on your box or in a cloud" },
  { cmd: "uc net", does: "a private mesh, VPNs and DNS" },
  { cmd: "uc kube", does: "Kubernetes on your hosts" },
  { cmd: "uc image", does: "build, move and run images" },
  { cmd: "uc db", does: "databases, ready to use" },
  { cmd: "uc mail", does: "your own mail server" },
  { cmd: "uc secret", does: "secrets, kept encrypted" },
  { cmd: "uc ws", does: "a dev container for each of you" },
];

export const FEATURES = [
  {
    tag: "uc",
    title: "Every tool, one command",
    body:
      "Machines, networks, Kubernetes, images, DNS, mail, storage, databases " +
      "and secrets, all behind uc. Your agent doesn't have to glue five " +
      "vendors together; it just runs the next command.",
  },
  {
    tag: "team",
    title: "Built for a team",
    body:
      "You and your agents use the same uc, on the same nodes, each with keys " +
      "of your own. What one of you sets up is there for everyone else, so " +
      "nobody works it out twice.",
  },
  {
    tag: "nodes",
    title: "Bring any node",
    body:
      "A box under your desk, a VPS, a rack in a shop or a cloud VM. Each one " +
      "runs one small daemon, usecoded, and that's all it needs.",
  },
  {
    tag: "mesh",
    title: "A private network between them",
    body:
      "Your nodes join a WireGuard mesh, so they talk to each other privately " +
      "wherever they are: at home, at the edge or in a cloud.",
  },
  {
    tag: "watch",
    title: "Looked after around the clock",
    body:
      "Each node's daemon puts drift right on its own. On Hosted, usecode.dev " +
      "also watches your nodes from outside, so when a whole cluster goes " +
      "quiet, someone hears about it before your users do.",
  },
  {
    tag: "open",
    title: "Open source, no node meter",
    body:
      "usecode is open source and freely distributed. Your own account is " +
      "free with two nodes or two hundred. You pay only for what we host or " +
      "look after for you, never per node.",
  },
];

export const STEPS = [
  { title: "Make an account", body: "An email and a password. Takes a few seconds, and it's free." },
  { title: "Hand your agent a key", body: "One line in Claude Code, Cursor, Codex or whatever you use." },
  { title: "Add your nodes", body: "Edge boxes or Kubernetes nodes. usecode.dev takes it from there." },
];

export const FAQS = [
  {
    q: "Is it really free?",
    a:
      "Yes. Every feature is open source and free, however many nodes you " +
      "add. You pay only if you'd like us to host the heavy parts, " +
      HOSTED_PRICE.amount + " " + HOSTED_PRICE.period + ", or to make sure " +
      "it doesn't break, which we work out together.",
  },
  {
    q: "What counts as a node?",
    a:
      "Any Linux machine you can run usecoded on: an edge box, a VM, a " +
      "dedicated server, or a node in your Kubernetes cluster.",
  },
  {
    q: "What happens to my nodes if I cancel?",
    a:
      "Your nodes keep running exactly as they are; they never depended on " +
      "us. Once the period you paid for ends, we stop hosting the monitoring, " +
      "audit log and backups, and your account goes back to Individual.",
  },
  {
    q: "Can I run it all myself instead?",
    a:
      "Sure, usecode is open source, and everything we host you can run " +
      "yourself. It just takes machines, storage and someone watching them. " +
      "If you'd rather we make sure it doesn't break, we're happy to talk.",
  },
];
