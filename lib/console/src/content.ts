// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

// The site's copy, as plain data, so the views stay about layout.

export const REPO_URL = "https://github.com/evgnomon/usecode";
export const CONTACT = "mailto:hg@evgnomon.org";

// What a seat on a team costs; the Stripe price behind it is STRIPE_PRICE_ID.
export const TEAM_PRICE = {
  amount: "$10",
  period: "per user a month",
};

export const PLANS = {
  individual: {
    name: "Individual",
    price: "Free",
    note: "For you and your agents, however many nodes you add.",
    points: [
      "Unlimited nodes, edge or Kubernetes",
      "Around-the-clock supervision that puts drift right",
      "A private WireGuard mesh between your nodes",
      "API access for your agents",
    ],
  },
  team: {
    name: "Team",
    price: TEAM_PRICE.amount,
    note: "Everything in Individual, for everyone on your team.",
    points: [
      "Everything in Individual",
      "Pay only for the people, never for the nodes",
      "Upgrades and our uptime are on us",
      "Cancel any time from your account",
    ],
  },
  enterprise: {
    name: "Enterprise",
    price: "Let's talk",
    note: "For bigger setups that need more than a card on file.",
    points: [
      "Everything in Team",
      "Help setting it up and running it, yours or ours",
      "Invoicing and terms that fit your company",
    ],
  },
};

// The opening of the landing page: why usecode exists, one stanza at a time.
export const MOTIVATION = {
  site: "usecode.dev",
  tagline: "Where intelligence uses code, and code makes intelligence.",
  stanzas: [
    [
      "Use code to think.",
      "Use code to build the base.",
      "Use code to run code.",
      "Use code to package it.",
      "Use code to deploy it.",
      "Use code to reuse it.",
    ],
    [
      "Use code to build the agent.",
      "Use code to run the agent.",
      "Use code to package the agent.",
      "Use code to deploy the agent.",
      "Use code to improve the agent.",
    ],
    ["Use code.", "Again.", "And again."],
  ],
  close: "This is the way intelligence compounds.",
};

export const FEATURES = [
  {
    title: "Bring any node",
    body:
      "A box under your desk, a VPS, a rack in a shop, or the nodes of your " +
      "Kubernetes cluster. Each one runs one small daemon, usecoded, and " +
      "that's all it needs.",
  },
  {
    title: "We keep an eye on them",
    body:
      "usecode.dev supervises your nodes around the clock. When something " +
      "drifts, a service stops or a node falls behind, it nudges things back " +
      "the right way, so you don't have to notice first.",
  },
  {
    title: "A private network between them",
    body:
      "Your nodes join a WireGuard mesh, so they talk to each other privately " +
      "wherever they are: at home, at the edge or in a cloud.",
  },
  {
    title: "Your agents get the keys",
    body:
      "Hand your AI agent an API key and it can reach your machines, run " +
      "tools and spin up what it needs, in the sentence you already typed.",
  },
  {
    title: "Open source, freely distributed",
    body:
      "usecode is open source and freely distributed under the HGL General " +
      "License, so you can read the code, run it and share it.",
  },
  {
    title: "Free for you, no node meter",
    body:
      "Your own account costs nothing, whether you add two nodes or two " +
      "hundred. Teams pay per person, never per node.",
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
      "For your own account, yes, however many nodes you add. Teams pay " +
      TEAM_PRICE.amount + " per user a month, and for an enterprise setup we " +
      "work it out together.",
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
      "Your team's nodes keep running exactly as they are. Once the period " +
      "you paid for ends, your account goes back to Individual.",
  },
  {
    q: "Can I run it all myself instead?",
    a:
      "Sure, usecode is open source. Running it yourself is on your own time; if " +
      "you'd like a hand setting it up for your team, that's a paid " +
      "engagement and we're happy to talk.",
  },
];
