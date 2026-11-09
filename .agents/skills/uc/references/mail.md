# uc mail

Two halves: reading and sending your own mail from this machine, and looking
after the Stalwart mail server on your cluster.

## Your mail

Accounts are in `~/.config/usecode/mail.yaml`:

```yaml
accounts:
  - name: work
    host: imap.example.com
    user: ada@example.com
    password_secret: mail.work   # a path in `uc secret`
    # port: 993
    # mailboxes: [INBOX, Archive]
    # from: Ada Lovelace <ada@example.com>
    # smtp_host: smtp.example.com   (default: host with imap. made smtp.)
    # smtp_port: 465                (587 for STARTTLS)
    # sent_mailbox: Sent
```

The password is `UC_MAIL_PASSWORD_<ACCOUNT>`, else `password_secret` in the
`uc secret` store, else asked for (which the agent can't answer).

```bash
uc mail pull                             # new mail of every account
uc mail pull work --last 200             # first pull of a big mailbox
uc mail view -n 10                       # the 10 newest, one line each
uc mail view -p -n 10                    # pull first
uc mail view -n 10 --full --json         # whole, as JSON: the way for the agent to read mail
uc mail view work/INBOX/1-42             # one message, by the id a listing shows
uc mail view -a work -n 5                # one account
```

Without `-n` or an id, `uc mail view` opens a picker, which needs a terminal:
from the agent always give `-n` or ids.

Sending and replying. Always pass `-b`, `-f` or stdin and `-y`, or it opens
`$EDITOR` or asks; and since mail can't be taken back, confirm the text with
the user first (`ask: true`):

```bash
uc mail send ada@example.com -s Report -b 'Here it is.' -A report.pdf -y
echo 'Done.' | uc mail send ada@example.com --cc bob@example.com -s Build -y
uc mail send -a work --to 'Ada <ada@example.com>' -s Notes -f notes.txt -y
uc mail view -r work/INBOX/1-42 -b 'Thanks, done.' -y
uc mail view -r work/INBOX/1-42 --all --no-quote -f reply.txt -y
```

A reply goes out from the account the mail came to unless `-a` says
otherwise.

## The mail server

Every command takes `--context NAME`. Stalwart is reached as the recovery
admin in `mail/mail-admin`, or `UC_MAIL_USER` and `UC_MAIL_PASSWORD`.

```bash
uc mail domain list
uc mail domain add example.com [--alias example.org]
uc mail domain catch-all example.com ada@example.com   # or --off
uc mail domain rm example.com            # once it has no accounts
uc mail account list [example.com]
uc mail account add bob@example.com --random --quota 5G
echo 'secret' | uc mail account add ada@example.com    # password from stdin without a terminal
uc mail account password ada@example.com
uc mail account alias add ada@example.com hello@example.com
uc mail account quota ada@example.com none
uc mail account rm bob@example.com       # deletes the account and all its mail
uc mail configure -C                     # DNS, reverse DNS, DKIM, DMARC: what would change
uc mail configure -y [example.com]       # fix it
uc mail settings ada@example.com         # IMAP/SMTP settings for a mail app
```

`configure` reads `mail_host`, `mail_node` and `mail_dmarc_policy` under
`kube.<context>` in the user config. Cloudflare needs `CLOUDFLARE_API_TOKEN`
(Zone:Read, DNS:Edit); Hetzner reverse DNS uses the cluster's token in
`kube-system/hcloud` or `HCLOUD_TOKEN`. Run it again after adding a domain
or a DKIM key, and after Stalwart moves to another worker.

`account rm` and `domain rm`: `ask: true`.
