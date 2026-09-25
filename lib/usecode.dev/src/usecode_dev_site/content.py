# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

"""Site copy. Kept as plain data so templates stay presentation-only."""

REPO_URL = "https://github.com/evgnomon/usecode"
LICENSE_NAME = "HGL General License"
LICENSE_URL = "http://evgnomon.org/docs/hgl"
SOURCE_LICENSE_URL = f"{REPO_URL}/blob/main/COPYING"

FEATURES = [
    {
        "title": "The agent stops at the dashboard",
        "body": (
            "Without provisioning tools in reach, your agent writes the app and "
            "then hands you a paragraph of instructions — and you go click "
            "through a provider console yourself. usecode exposes provisioning "
            "as MCP tools the agent discovers on its own, so the sentence you "
            "already typed is the last thing you have to type."
        ),
    },
    {
        "title": "Every provider renames the same six things",
        "body": (
            "Every provider has its own word for a server, a volume, a network, "
            "a DNS record — and everything you wrote against the first one has "
            "to be written again for the second. Describe it once here and "
            "moving a workload is a credential change."
        ),
    },
    {
        "title": "You find out what it did from the bill",
        "body": (
            "An agent that applies straight to a live account tells you what it "
            "destroyed afterwards. Every usecode tool call reports what it will "
            "create, change or destroy first, in chat, before it touches an "
            "account."
        ),
    },
    {
        "title": "The part you need sits behind a quote",
        "body": (
            "Open-core tooling keeps the provider you actually run, or the audit "
            "trail your auditor asks for, on the other side of a quote. Every "
            "tool and every provider here is published under the "
            "HGL General License — the hosted deployment is built from the same "
            "tag you can download."
        ),
    },
    {
        "title": "Your cloud keys, on someone else's roadmap",
        "body": (
            "Hand root credentials to a SaaS and their outage, their breach and "
            "their pricing change are yours too. Run usecode on your machine and "
            "provider keys are read from your environment and never leave it — "
            "and if you later let us run it, it is the identical code path."
        ),
    },
]

PROVIDERS = [
    "Hetzner Cloud",
    "DigitalOcean",
    "AWS",
    "Google Cloud",
    "Azure",
    "Vultr",
    "Linode",
    "Bare metal (SSH)",
]

EDITIONS = [
    {
        "name": "Self-hosted",
        "price": "Free",
        "period": "forever",
        "badge": None,
        "tagline": "All of it, on your machine. No trial clock to run out.",
        "cta": {"label": "Get started", "href": "/getting-started"},
        "featured": False,
        "points": [
            "All providers, all MCP tools, all of the source",
            "Credential vault, audit log and drift detection included",
            "Runs over stdio locally, or as an HTTP endpoint you operate",
            "HGL licensed — nobody can take this build away from you",
        ],
    },
    {
        "name": "Team",
        "price": "$29",
        "period": "per user / month",
        "badge": "Hosted",
        "tagline": "The same build on our machines, so the 3am page isn't yours.",
        "cta": {"label": "Start a trial", "href": "/team"},
        "featured": True,
        "points": [
            "An endpoint still answering when your laptop is asleep",
            "Upgrades, backups, restore drills and HA stop being your weekend",
            "An SLA and a support commitment in writing, not goodwill",
            "Retained audit history and point-in-time state restore",
            "Funds the release everyone else keeps downloading free",
        ],
    },
    {
        "name": "Support & warranty",
        "price": "Custom",
        "period": "per organisation / year",
        "badge": "Self-hosted",
        "tagline": "Keep your infrastructure where it is; stop carrying it alone.",
        "cta": {"label": "Talk to us", "href": "mailto:hg@evgnomon.org"},
        "featured": False,
        "points": [
            "Named support with agreed response times on your deployment",
            "Written warranty and indemnity in place of the AS-IS disclaimer",
            "Upgrade and migration review before a version bump surprises you",
            "Priority on the fixes and providers your estate depends on",
        ],
    },
]

# What the paid tiers actually sell. None of these are code — by design.
HOSTED_VALUES = [
    {
        "title": "It reconciles only while your laptop is awake",
        "body": (
            "A closed lid, a dropped wifi, a reboot for updates — each one is a "
            "scheduled reconciliation that never ran and drift you find out "
            "about later. A twenty-minute provision needs a process that does "
            "none of those things."
        ),
    },
    {
        "title": "The upgrade lands on your Saturday",
        "body": (
            "Version bumps, database backups, restore drills, failover, "
            "patching. All of it is work you can do yourself with exactly the "
            "same software — the subscription is you not doing it."
        ),
    },
    {
        "title": "AS IS means you",
        "body": (
            "The public release ships with no warranty, as HGL requires, so when "
            "it goes wrong there is nobody on the other end of it. A paid plan "
            "puts an SLA, response times and indemnity there instead — the one "
            "thing a licence genuinely cannot hand you for free."
        ),
    },
    {
        "title": "An untested backup is a rumour",
        "body": (
            "These tools tear down infrastructure on request, and an untested "
            "backup is a rumour. Paid plans come with retained audit history and "
            "point-in-time restore of provisioning state, operated by people who "
            "rehearse the restore."
        ),
    },
    {
        "title": "A Postgres to run, just to run a provisioner",
        "body": (
            "Self-hosting means a database to keep alive, certificates to renew "
            "and an upgrade path to own. Hosted is a URL and an API key — and on "
            "the day that trade stops being worth it, export and self-host the "
            "same build."
        ),
    },
    {
        "title": "Abandoned tooling is a migration you pay for later",
        "body": (
            "Nothing is held back to make a paid tier worth buying, so "
            "subscriptions are the only thing funding the work. Unmaintained "
            "infrastructure tooling is a migration you pay for later."
        ),
    },
]

SELF_HOST_STEPS = [
    {
        "title": "Bring up the same stack we run",
        "body": (
            "No reconstruction from docs: deploy/compose.yml is the deployment — "
            "API instances behind Caddy, sharded Postgres pair underneath. It is "
            "the file our own hosted environment is built from."
        ),
        "code": (
            "git clone https://github.com/evgnomon/usecode\n"
            "cd usecode/deploy\n"
            "docker compose up -d"
        ),
        "lang": "sh",
    },
    {
        "title": "Point your agents at your own endpoint",
        "body": (
            "Identical client config to the hosted one — your host in the URL, a "
            "key you issued yourself. Nothing to port if you change your mind."
        ),
        "code": None,
        "lang": None,
    },
    {
        "title": "Now the pager is yours",
        "body": (
            "Backups, restore drills, certificate renewal, failover and version "
            "bumps land on you from here. That work — never the software — is "
            "the whole of what a paid plan removes."
        ),
        "code": None,
        "lang": None,
    },
]

LICENSE_POINTS = [
    "Copy it, modify it and run it — commercially, internally, at any scale.",
    "Host it for yourself or your own customers; no seat count to declare.",
    "Distribute a derivative and it travels with its source, under HGL.",
    "The copyright notice and the licence text stay with the work.",
]

INSTALL_STEPS = [
    {
        "title": "Install",
        "body": "usecode ships as a Python package. uv is the quickest way in.",
        "code": "uv tool install usecode-mcp",
        "lang": "sh",
    },
    {
        "title": "Give it provider credentials",
        "body": (
            "Export the tokens for the clouds you want to reach. Skip a provider "
            "and it simply never appears — usecode only offers what it finds "
            "credentials for."
        ),
        "code": (
            "export HCLOUD_TOKEN=...\n"
            "export DIGITALOCEAN_TOKEN=...\n"
            "export AWS_ACCESS_KEY_ID=...\n"
            "export AWS_SECRET_ACCESS_KEY=..."
        ),
        "lang": "sh",
    },
    {
        "title": "Point your agent at it",
        "body": (
            "Pick your agent below. After it reconnects, ask for infrastructure "
            "in your own words — the tools are discovered automatically, so "
            "there is no syntax to go and learn."
        ),
        "code": None,
        "lang": None,
    },
]

AGENTS = [
    {
        "id": "claude-code",
        "name": "Claude Code",
        "note": "Adds usecode to the current project scope.",
        "lang": "sh",
        "code": "claude mcp add usecode -- uvx usecode-mcp",
    },
    {
        "id": "claude-desktop",
        "name": "Claude Desktop",
        "note": "Edit claude_desktop_config.json, then restart the app.",
        "lang": "json",
        "code": """{
  "mcpServers": {
    "usecode": {
      "command": "uvx",
      "args": ["usecode-mcp"]
    }
  }
}""",
    },
    {
        "id": "cursor",
        "name": "Cursor",
        "note": "Write .cursor/mcp.json in the project (or ~/.cursor/mcp.json globally).",
        "lang": "json",
        "code": """{
  "mcpServers": {
    "usecode": {
      "command": "uvx",
      "args": ["usecode-mcp"]
    }
  }
}""",
    },
    {
        "id": "vscode",
        "name": "VS Code / Copilot",
        "note": "Write .vscode/mcp.json, then Start the server from the editor.",
        "lang": "json",
        "code": """{
  "servers": {
    "usecode": {
      "type": "stdio",
      "command": "uvx",
      "args": ["usecode-mcp"]
    }
  }
}""",
    },
    {
        "id": "zed",
        "name": "Zed",
        "note": "Add a context server in settings.json.",
        "lang": "json",
        "code": """{
  "context_servers": {
    "usecode": {
      "source": "custom",
      "command": "uvx",
      "args": ["usecode-mcp"]
    }
  }
}""",
    },
    {
        "id": "team",
        "name": "Team (hosted)",
        "note": "No local install — the hosted endpoint works with any MCP client that speaks HTTP.",
        "lang": "json",
        "code": """{
  "mcpServers": {
    "usecode": {
      "type": "http",
      "url": "https://mcp.usecode.dev",
      "headers": { "Authorization": "Bearer ${USECODE_API_KEY}" }
    }
  }
}""",
    },
]

EXAMPLE_PROMPTS = [
    "Spin up a staging environment with a small app server and a Postgres box.",
    "Give the API server twice the memory and move it to Frankfurt.",
    "What is running in production right now, and what does it cost?",
    "Tear down everything tagged experiment.",
]

FAQS = [
    {
        "q": "Do I have to change how I prompt?",
        "a": (
            "No — and that is the point. Provisioning shows up as tools your "
            "agent already knows how to reach for, so you never end up "
            "maintaining a prompt that encodes one provider's dialect and breaks "
            "when you leave it."
        ),
    },
    {
        "q": "What does the hosted version have that the public release doesn't?",
        "a": (
            "No code at all. Same tag, same tools, same credential vault, same "
            "audit log, same drift detection. What you buy is an endpoint that "
            "stays up, someone carrying the pager for it, and an agreement that "
            "says so in writing — none of which arrive by downloading anything."
        ),
    },
    {
        "q": "Then why would I pay?",
        "a": (
            "Because free software still has an operator, and by default that is "
            "you: the always-on endpoint, the tested backups, the restore "
            "drills, the upgrades, the 3am page. Someone's time pays for those. "
            "If yours is cheaper than the subscription, self-host — genuinely."
        ),
    },
    {
        "q": "What does the HGL licence let me do?",
        "a": (
            "Use it, modify it and run it commercially, including hosting it for "
            "your own customers, with nothing to declare and no seat count to "
            "report. The condition is on distribution: ship a derivative and it "
            "travels with its full source under the same licence, copyright "
            "notice intact."
        ),
    },
    {
        "q": "Can I pay you without hosting with you?",
        "a": (
            "Yes — that is the support and warranty plan. Otherwise the AS-IS "
            "disclaimer stands and the risk on your own deployment has no "
            "counterparty; this attaches named support, response times and an "
            "indemnity to it. HGL sets those aside precisely as things that may "
            "be offered for a fee."
        ),
    },
    {
        "q": "Where do my cloud credentials live?",
        "a": (
            "Wherever you put the server — you are never forced to hand root "
            "credentials to a third party to get started. Locally, keys are read "
            "from your environment and stay there. On your own hardware, in your "
            "vault. On the hosted plan, in that same vault implementation on our "
            "machines, scoped per member."
        ),
    },
    {
        "q": "What happens to my data if I stop paying?",
        "a": (
            "You export it and point the same build at your own host. There is "
            "no proprietary half to lose access to, so leaving costs you an "
            "afternoon rather than a migration project."
        ),
    },
    {
        "q": "Can I use it with an agent that isn't listed?",
        "a": (
            "Yes — no client here is a dead end. Anything that speaks MCP works: "
            "run usecode-mcp over stdio, or point the client at an HTTP "
            "endpoint, ours or your own."
        ),
    },
]
