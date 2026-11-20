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
      "Every feature, freely distributed.",
      "Unlimited nodes, edge or Kubernetes.",
      "A private WireGuard mesh between your nodes.",
      "API access for your agents.",
    ],
  },
  hosted: {
    name: "Hosted",
    price: HOSTED_PRICE.amount,
    note: "The same usecode, with the heavy parts run by us, outside your cluster.",
    points: [
      "Monitoring and alerts, watched from outside your nodes.",
      "An audit log of what changed, kept where no one can rewrite it.",
      "Backups stored off your nodes.",
      "Upgrades and uptime of what we host are on us.",
    ],
  },
  enterprise: {
    name: "Enterprise",
    price: "Let's talk",
    note: PROMISE,
    points: [
      "Everything in Hosted.",
      "We review what your agents plan to change in production.",
      "We keep it working through upgrades and changes upstream.",
      "Someone who knows your setup answers when it breaks.",
      "A contract, invoicing and terms that fit your company.",
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
  tagline: "Use Code and/or Make AGI.",
  intro: [
    "usecode is part of HGL/z: one platform as code, under HGL, that every " +
      "contribution merges back into. HGL/z is the AGI we can all build, and " +
      "own together.",
    "Start with a coding agent that comes with everything real software needs: " +
      "build, test, deploy, hosting and scaling, already wired in. Nothing to install. " +
      "Open a browser, say what you want, and get an app that’s live today and ready for whatever comes next.",
  ],
  // Highlighted above the pillars, a link to the code.
  openSource: "Freely distributed.",
  // The three things that set it apart, as a row under the intro.
  pillars: ["Batteries included.", "Runs entirely in your browser.", "Convention over configuration."],
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
  close: "Built so you never start over.",
  kicker: "How a prompt gets you there.",
  body:
    "Most software gets rebuilt on the way up: the laptop demo moves to a " +
    "hosting service, that moves to a cloud, and the cloud gets a platform " +
    "team. usecode skips the rebuilds. From your first prompt, your agent " +
    "works on what big companies run on: real Linux nodes, a private " +
    "WireGuard mesh, Kubernetes, databases, DNS, mail and secrets. When you " +
    "grow, you add nodes. You never have to move to a new stack.",
  team:
    "It stays up because every node runs usecoded, a small daemon that keeps " +
    "its host set up and fixes drift on its own. Your setup is config in your " +
    "own repo, so moving to another cloud or your own racks means applying " +
    "the same config somewhere else. And the work adds up: every tool is a uc " +
    "command that you, your team and your agents share, and when one is " +
    "missing, the agent adds it, so the next job starts further ahead.",
  ship: "Not just another coding agent. Everything a company runs on, and an agent that keeps adding to it.",
};

const BLOG = "https://evgnomon.org/posts";

/**
 * Why "Use Code and/or Make AGI.", under the hero: where AI stands now, where it goes, who
 * may own it, and the principles HGL/z is built on. From the blog at
 * evgnomon.org, linked in `reading`.
 */
export const AGI = {
  kicker: "Why we make AGI.",
  title: "AGI is coming either way. What matters is whose it is.",
  now: {
    heading: "Now",
    points: [
      "AI is turning into infrastructure. Once hospitals, payments and offices " +
        "run on it, nobody can pull the plug, not even the people meant to " +
        "regulate it. Dependence is enough to make the off switch useless.",
      "Every check on AI is a task, and every task ends up with AI: it writes " +
        "the proposal, sums it up for the approver and audits the result. " +
        "Limits only hold if someone enforces them, and enforcing is a task too.",
      "Labor is losing its scarcity, and with it the say it gave us. Value " +
        "moves to whoever owns what stays scarce: energy, chips and access to " +
        "the models, today in a handful of companies.",
    ],
  },
  next: {
    heading: "Next",
    points: [
      "If a harmful AGI is possible, someone will try to build it. Stopping " +
        "halfway doesn’t make that go away; it only hands the most powerful " +
        "AI to whoever ignored the limits.",
      "A guardian AI doesn’t solve it either: who guards the guardian? So the " +
        "answer is an AGI that is as powerful as possible, needs no guardian " +
        "and is genuine, not because anyone confirmed it, but because that is " +
        "what it is.",
      "That AGI is built in the open, by everyone who uses it, so the most " +
        "powerful AI is the one that belongs to all of us.",
    ],
  },
  // The line the section is about, set apart.
  owner:
    "AGI is not something one entity builds and owns, unless that entity is " +
    "no one and/or everyone.",
  ownerNote:
    "No one owning it doesn’t mean nobody holds the controls; something " +
    "without an owner is taken by whoever is quickest. HGL/z has owners: " +
    "everyone who builds it. What nobody owns is its nature.",
  principlesTitle: "How we get there.",
  principles: [
    {
      tag: "HGL",
      title: "Open, and it stays open.",
      body:
        "Anyone may copy and change it. Once you distribute it, the whole " +
        "source goes to every user, free, under HGL, with no extra " +
        "restriction. No fork can close what it has to hand over.",
    },
    {
      tag: "z",
      title: "One branch, never built twice.",
      body:
        "Every fork starts from Z and every useful fork merges back. Need a " +
        "new function? Add the next block instead of starting a new project. " +
        "Each of us invests a little and gains everything already built.",
    },
    {
      tag: "protocol",
      title: "Built on, not taken over.",
      body:
        "Many repositories work as one when they speak the same interface " +
        "and protocol, wherever their code lives. You build on HGL/z the way " +
        "you build on a protocol, not to take control of it.",
    },
    {
      tag: "use",
      title: "Use is develop.",
      body:
        "You develop what you use and use what you develop. When a tool is " +
        "missing, you or your agent adds it to uc, and the next person " +
        "starts further ahead.",
    },
    {
      tag: "nodes",
      title: "No single off switch.",
      body:
        "It runs on your own nodes, at home, at the edge or in any cloud. " +
        "One switch for everyone is power over everyone, so there isn’t one " +
        "to capture.",
    },
    {
      tag: "btc",
      title: "Protected by BTC.",
      body:
        "Building HGL/z is paid in BTC, money no printer can dilute and no " +
        "one can easily take. It secures the time to build more, without " +
        "giving up anyone’s sovereignty, privacy or security.",
    },
  ],
  close: "Do HGL/z and/or BTC. Your choice.",
  reading: [
    { title: "HGL/z", href: `${BLOG}/hglz/` },
    { title: "Genuine AI", href: `${BLOG}/genuine-ai/` },
    { title: "Losing the Bottleneck", href: `${BLOG}/losing-the-bottleneck/` },
    { title: "The Printer Problem", href: `${BLOG}/printer-problem/` },
  ],
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
    title: "Every tool, one command.",
    body:
      "Machines, networks, Kubernetes, images, DNS, mail, storage, databases " +
      "and secrets, all behind uc. Your agent doesn't have to glue five " +
      "vendors together; it just runs the next command.",
  },
  {
    tag: "team",
    title: "Built for a team.",
    body:
      "You and your agents use the same uc, on the same nodes, each with keys " +
      "of your own. What one of you sets up is there for everyone else, so " +
      "nobody works it out twice.",
  },
  {
    tag: "nodes",
    title: "Bring any node.",
    body:
      "A box under your desk, a VPS, a rack in a shop or a cloud VM. Each one " +
      "runs one small daemon, usecoded, and that's all it needs.",
  },
  {
    tag: "mesh",
    title: "A private network between them.",
    body:
      "Your nodes join a WireGuard mesh, so they talk to each other privately " +
      "wherever they are: at home, at the edge or in a cloud.",
  },
  {
    tag: "watch",
    title: "Looked after around the clock.",
    body:
      "Each node's daemon puts drift right on its own. On Hosted, usecode.dev " +
      "also watches your nodes from outside, so when a whole cluster goes " +
      "quiet, someone hears about it before your users do.",
  },
  {
    tag: "open",
    title: "Freely distributed, no node meter.",
    body:
      "usecode is freely distributed. Your own account is " +
      "free with two nodes or two hundred. You pay only for what we host or " +
      "look after for you, never per node.",
  },
];

export const STEPS = [
  { title: "Make an account.", body: "An email and a password. Takes a few seconds, and it's free." },
  { title: "Put an agent to work.", body: "uc comes with its own agent. Or hand a key to Claude Code, Cursor, Codex or whatever you already use." },
  { title: "Add your nodes.", body: "Edge boxes or Kubernetes nodes. usecode.dev takes it from there." },
];

export const FAQS = [
  {
    q: "What does “Use Code and/or Make AGI” mean?",
    a:
      "HGL/z is AGI: one platform as code that observes, decides and acts, " +
      "built by everyone who uses it, in one branch everything merges back " +
      "into. usecode is the part you can use today, and every tool you or " +
      "your agents add makes it more capable for everyone.",
  },
  {
    q: "Who owns it?",
    a:
      "Everyone who builds it, and no one alone. The code is under HGL, so " +
      "whoever distributes it hands over the whole source, free. Not us, " +
      "not any company, can close it or change what it is.",
  },
  {
    q: "Is it really free?",
    a:
      "Yes. Every feature is freely distributed, however many nodes you " +
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
      "Sure, usecode is freely distributed, and everything we host you can run " +
      "yourself. It just takes machines, storage and someone watching them. " +
      "If you'd rather we make sure it doesn't break, we're happy to talk.",
  },
];
