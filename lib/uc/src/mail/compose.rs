// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc mail send` and the replies of `uc mail view`: write mail and send it.
//!
//! A [`Draft`] is new ([`Draft::default`]) or a reply to a message `uc mail
//! pull` fetched ([`Draft::reply`]). [`Compose`] are the flags both commands
//! take: what they give goes into the draft, and [`run`] either sends it as
//! it is, or first opens it in `$EDITOR` as a [`Draft::text`] (headers, a
//! blank line, the message) and asks before sending. It goes out over SMTP
//! with the account's password, see [`super::inbox::Account`], and a copy
//! goes to its `sent_mailbox` when it has one.

use super::inbox::{self, Account, Config, Secrets};
use anyhow::{Context, Result, bail};
use mail_parser::MessageParser;
use mail_send::SmtpClientBuilder;
use mail_send::mail_builder::MessageBuilder;
use mail_send::mail_builder::headers::address::Address as BuilderAddress;
use std::fs;
use std::io::{BufRead, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

/// The flags of `uc mail send` and of replying with `uc mail view`.
#[derive(clap::Args, Debug, Default)]
pub struct Compose {
    /// Send from this account [default: the reply's, else the first one];
    /// `uc mail view` lists only its mail.
    #[arg(short, long, value_name = "NAME")]
    pub account: Option<String>,
    /// To these, e.g. "Ada <ada@example.com>"; repeat it or use commas.
    #[arg(long, value_name = "ADDRESS")]
    pub to: Vec<String>,
    /// Copy these in.
    #[arg(long, value_name = "ADDRESS")]
    pub cc: Vec<String>,
    /// Hidden from the others.
    #[arg(long, value_name = "ADDRESS")]
    pub bcc: Vec<String>,
    #[arg(short, long)]
    pub subject: Option<String>,
    /// The message [default: stdin when it is not a terminal, else written
    /// in $EDITOR].
    #[arg(short, long, value_name = "TEXT", conflicts_with = "body_file")]
    pub body: Option<String>,
    /// The message from this file, - for stdin.
    #[arg(short = 'f', long, value_name = "FILE")]
    pub body_file: Option<PathBuf>,
    /// Attach this file; repeat it for more.
    #[arg(short = 'A', long, value_name = "FILE")]
    pub attach: Vec<PathBuf>,
    /// Open it in $EDITOR, even with the message given.
    #[arg(short, long)]
    pub edit: bool,
    /// Send it without asking, after the editor too.
    #[arg(short, long)]
    pub yes: bool,
}

/// An address and the name on it.
#[derive(Clone, Debug, PartialEq)]
pub struct Mailbox {
    pub name: Option<String>,
    pub address: String,
}

impl std::fmt::Display for Mailbox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.name {
            Some(name) if name.contains(|c: char| ",;<>\"@".contains(c)) => {
                write!(f, "\"{}\" <{}>", name.replace('"', ""), self.address)
            }
            Some(name) => write!(f, "{name} <{}>", self.address),
            None => write!(f, "{}", self.address),
        }
    }
}

impl Mailbox {
    fn is(&self, other: &Mailbox) -> bool {
        self.address.eq_ignore_ascii_case(&other.address)
    }
}

/// `Ada <ada@example.com>, bob@example.com` as mailboxes.
pub fn parse_addresses(text: &str) -> Result<Vec<Mailbox>> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let header = format!("To: {}\r\n\r\n", text.replace(['\r', '\n'], " "));
    let message = MessageParser::default()
        .parse(header.as_bytes())
        .with_context(|| format!("{text:?} is not an address"))?;
    let mut mailboxes = Vec::new();
    for addr in message.to().into_iter().flat_map(|a| a.iter()) {
        let address = addr.address().unwrap_or_default().trim();
        if !address.contains('@') || address.contains(char::is_whitespace) {
            bail!("{text:?} is not an address, like Ada <ada@example.com>");
        }
        mailboxes.push(Mailbox {
            name: addr
                .name()
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(String::from),
            address: address.to_string(),
        });
    }
    if mailboxes.is_empty() {
        bail!("{text:?} is not an address, like Ada <ada@example.com>");
    }
    Ok(mailboxes)
}

fn join(mailboxes: &[Mailbox]) -> String {
    mailboxes
        .iter()
        .map(Mailbox::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Who `account` sends as.
pub fn sender(account: &Account) -> Result<Mailbox> {
    let from = account.from.as_deref().unwrap_or(&account.user);
    let mut from =
        parse_addresses(from).with_context(|| format!("the from of account {}", account.name))?;
    if from.len() != 1 {
        bail!("account {} has more than one from", account.name);
    }
    Ok(from.remove(0))
}

/// A message to send.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Draft {
    pub to: Vec<Mailbox>,
    pub cc: Vec<Mailbox>,
    pub bcc: Vec<Mailbox>,
    pub subject: String,
    pub body: String,
    pub attachments: Vec<PathBuf>,
    /// The Message-ID it replies to.
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

impl Draft {
    /// A reply from `me` to the message `id`: to whoever it asks replies to
    /// go to, and with `all` also to everyone else it went to, the message
    /// quoted below.
    pub fn reply(id: &str, me: &Mailbox, all: bool) -> Result<Draft> {
        Draft::reply_to(&inbox::raw(id)?, me, all).with_context(|| format!("replying to {id}"))
    }

    /// [`Draft::reply`] to the message `raw`.
    pub fn reply_to(raw: &[u8], me: &Mailbox, all: bool) -> Result<Draft> {
        let message = MessageParser::default()
            .parse(raw)
            .context("not a mail message")?;
        let mailboxes = |address: Option<&mail_parser::Address>| -> Vec<Mailbox> {
            address
                .into_iter()
                .flat_map(|a| a.iter())
                .filter_map(|addr| {
                    Some(Mailbox {
                        name: addr.name().map(|n| n.trim().to_string()),
                        address: addr.address()?.trim().to_string(),
                    })
                })
                .filter(|m| m.address.contains('@'))
                .collect()
        };
        let from = mailboxes(message.from());
        let mut to = match mailboxes(message.reply_to()) {
            reply_to if !reply_to.is_empty() => reply_to,
            _ => from.clone(),
        };
        if to.iter().all(|m| m.is(me)) {
            // A reply to our own message goes to whom it went to.
            to = mailboxes(message.to());
        }
        let mut cc = Vec::new();
        if all {
            for m in mailboxes(message.to())
                .into_iter()
                .chain(mailboxes(message.cc()))
            {
                if !m.is(me) && !to.iter().chain(&cc).any(|o| o.is(&m)) {
                    cc.push(m);
                }
            }
        }
        let subject = message.subject().unwrap_or_default().trim();
        let subject = match subject.get(..3) {
            Some(re) if re.eq_ignore_ascii_case("re:") => subject.to_string(),
            _ => format!("Re: {subject}"),
        };
        let mut references: Vec<String> = message
            .references()
            .as_text_list()
            .unwrap_or_default()
            .iter()
            .map(|r| r.to_string())
            .collect();
        let in_reply_to = message.message_id().map(String::from);
        references.extend(in_reply_to.clone());
        let wrote = match (message.date(), from.first()) {
            (Some(date), Some(from)) => format!("On {}, {from} wrote:", date.to_rfc822()),
            (None, Some(from)) => format!("{from} wrote:"),
            _ => "You wrote:".to_string(),
        };
        let quoted = message
            .body_text(0)
            .unwrap_or_default()
            .trim_end()
            .lines()
            .map(|line| match line {
                "" => ">".to_string(),
                line if line.starts_with('>') => format!(">{line}"),
                line => format!("> {line}"),
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(Draft {
            to,
            cc,
            subject,
            body: format!("\n\n{wrote}\n{quoted}\n"),
            in_reply_to,
            references,
            ..Draft::default()
        })
    }

    /// The draft to edit: `From`, `To`, `Cc`, `Bcc`, `Subject` and one
    /// `Attach` per file, a blank line, and the message.
    pub fn text(&self, from: &Mailbox, account: &str) -> String {
        let mut text = format!(
            "# Write the message below the blank line. Lines starting with #\n\
             # are left out, an empty To: sends nothing. One Attach: per file.\n\
             # From: {from} ({account})\n\
             To: {}\nCc: {}\nBcc: {}\nSubject: {}\n",
            join(&self.to),
            join(&self.cc),
            join(&self.bcc),
            self.subject
        );
        for path in &self.attachments {
            text += &format!("Attach: {}\n", path.display());
        }
        if self.attachments.is_empty() {
            text += "Attach: \n";
        }
        text + "\n" + &self.body
    }

    /// Reads back [`Draft::text`], keeping what it can't say, the reply's
    /// ids.
    pub fn read_text(&self, text: &str) -> Result<Draft> {
        let mut draft = Draft {
            in_reply_to: self.in_reply_to.clone(),
            references: self.references.clone(),
            ..Draft::default()
        };
        let (head, body) = match text.split_once("\n\n") {
            Some((head, body)) => (head, body),
            None => (text, ""),
        };
        let mut fields: Vec<(String, String)> = Vec::new();
        for line in head.lines() {
            if line.starts_with('#') {
                continue;
            }
            if line.starts_with([' ', '\t'])
                && let Some((_, value)) = fields.last_mut()
            {
                *value += " ";
                *value += line.trim();
                continue;
            }
            let Some((name, value)) = line.split_once(':') else {
                bail!(
                    "{line:?} is not a header like Subject: hello; the message goes after a blank line"
                );
            };
            fields.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
        for (name, value) in fields {
            match name.as_str() {
                "to" => draft.to.extend(parse_addresses(&value)?),
                "cc" => draft.cc.extend(parse_addresses(&value)?),
                "bcc" => draft.bcc.extend(parse_addresses(&value)?),
                "subject" => draft.subject = value,
                "attach" if value.is_empty() => {}
                "attach" => draft.attachments.push(expand_home(&value)),
                _ => bail!("unknown header {name}:, these are To, Cc, Bcc, Subject and Attach"),
            }
        }
        draft.body = body.to_string();
        Ok(draft)
    }

    /// Adds what the flags give: more recipients and attachments, the
    /// subject, and the message above a reply's quote.
    fn apply(&mut self, flags: &Compose, body: Option<String>) -> Result<()> {
        for (list, given) in [
            (&mut self.to, &flags.to),
            (&mut self.cc, &flags.cc),
            (&mut self.bcc, &flags.bcc),
        ] {
            for text in given {
                list.extend(parse_addresses(text)?);
            }
        }
        if let Some(subject) = &flags.subject {
            self.subject = subject.clone();
        }
        self.attachments.extend(flags.attach.iter().cloned());
        if let Some(body) = body {
            self.body = match self.in_reply_to {
                Some(_) => format!("{}{}", body.trim_end(), self.body),
                None => body,
            };
        }
        Ok(())
    }

    fn check(&self) -> Result<()> {
        if self.to.is_empty() && self.cc.is_empty() && self.bcc.is_empty() {
            bail!("no one to send it to, give --to");
        }
        for path in &self.attachments {
            if !path.is_file() {
                bail!("no file {} to attach", path.display());
            }
        }
        Ok(())
    }

    /// What `from` sends: the message with its attachments, without Bcc.
    pub fn build(&self, from: &Mailbox) -> Result<Vec<u8>> {
        let addresses = |list: &[Mailbox]| -> BuilderAddress<'static> {
            list.iter()
                .map(|m| match &m.name {
                    Some(name) => BuilderAddress::from((name.clone(), m.address.clone())),
                    None => BuilderAddress::from(m.address.clone()),
                })
                .collect::<Vec<_>>()
                .into()
        };
        let mut message = MessageBuilder::new()
            .from(addresses(std::slice::from_ref(from)))
            .subject(self.subject.clone())
            .text_body(self.body.replace("\r\n", "\n").replace('\n', "\r\n"));
        if !self.to.is_empty() {
            message = message.to(addresses(&self.to));
        }
        if !self.cc.is_empty() {
            message = message.cc(addresses(&self.cc));
        }
        if let Some(id) = &self.in_reply_to {
            message = message.in_reply_to(id.clone());
        }
        if !self.references.is_empty() {
            message = message.references(self.references.clone());
        }
        for path in &self.attachments {
            let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "attachment".into());
            let mime = mime_guess::from_path(path)
                .first_or_octet_stream()
                .to_string();
            message = message.attachment(mime, name, bytes);
        }
        Ok(message.write_to_vec()?)
    }

    /// Who gets it, Bcc too.
    fn recipients(&self) -> Vec<String> {
        let mut all: Vec<String> = Vec::new();
        for m in self.to.iter().chain(&self.cc).chain(&self.bcc) {
            if !all.iter().any(|a| a.eq_ignore_ascii_case(&m.address)) {
                all.push(m.address.clone());
            }
        }
        all
    }

    fn summary(&self, from: &Mailbox) -> String {
        let mut out = format!("From:    {from}\nTo:      {}\n", join(&self.to));
        if !self.cc.is_empty() {
            out += &format!("Cc:      {}\n", join(&self.cc));
        }
        if !self.bcc.is_empty() {
            out += &format!("Bcc:     {}\n", join(&self.bcc));
        }
        out += &format!("Subject: {}\n", self.subject);
        for path in &self.attachments {
            let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            out += &format!("Attach:  {} ({size} bytes)\n", path.display());
        }
        out
    }
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(rest),
        None => PathBuf::from(path),
    }
}

/// The account to send from: `name`, else the reply's, else the first.
pub fn account<'a>(
    config: &'a Config,
    name: Option<&str>,
    reply: Option<&str>,
) -> Result<&'a Account> {
    let reply_account = reply.and_then(|id| id.split('/').next());
    match name.or(reply_account) {
        Some(name) => Ok(config.select(&[name.to_string()])?[0]),
        None => config
            .accounts
            .first()
            .with_context(|| format!("no accounts in {}", inbox::config_path().display())),
    }
}

/// Sends `draft` from `account` as `flags` say: as it is when the message
/// is given (or piped in), else after writing it in `$EDITOR`.
pub async fn run(account: &Account, mut draft: Draft, flags: &Compose) -> Result<()> {
    let terminal = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let body = match (&flags.body, &flags.body_file) {
        (Some(body), _) => Some(body.clone()),
        (None, Some(path)) if path == Path::new("-") => Some(read_stdin()?),
        (None, Some(path)) => {
            Some(fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?)
        }
        (None, None) if !std::io::stdin().is_terminal() => Some(read_stdin()?),
        (None, None) => None,
    };
    let edit = flags.edit || body.is_none();
    draft.apply(flags, body)?;
    let from = sender(account)?;
    if edit {
        if !terminal {
            bail!("no message: give --body or --body-file, or pipe it in");
        }
        match edit_draft(account, &from, draft, flags.yes)? {
            Some(edited) => draft = edited,
            None => return Ok(()),
        }
    }
    draft.check()?;
    send(account, &from, &draft).await
}

/// Lets the draft be written in `$EDITOR` and asks to send it, until it is
/// sent (`Some`) or put off (`None`), when it stays in the drafts dir.
fn edit_draft(account: &Account, from: &Mailbox, draft: Draft, yes: bool) -> Result<Option<Draft>> {
    let dir = inbox::data_dir().join("drafts");
    fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    let path = dir.join(format!(
        "{}-{}.txt",
        chrono::Local::now().format("%Y%m%d-%H%M%S"),
        std::process::id()
    ));
    fs::write(&path, draft.text(from, &account.name))
        .with_context(|| format!("writing {}", path.display()))?;
    let kept = |why: &str| {
        eprintln!("{why}, the draft is in {}", path.display());
        Ok(None)
    };
    loop {
        editor(&path).with_context(|| format!("the draft is in {}", path.display()))?;
        let text = fs::read_to_string(&path)?;
        let edited = match draft.read_text(&text).and_then(|d| d.check().map(|()| d)) {
            Ok(edited) => edited,
            Err(err) => {
                eprintln!("{err:#}");
                match ask("[e]dit it again or [q]uit? ")? {
                    'e' => continue,
                    _ => return kept("Not sent"),
                }
            }
        };
        if yes {
            fs::remove_file(&path).ok();
            return Ok(Some(edited));
        }
        eprint!("\n{}", edited.summary(from));
        match ask("Send it? [y]es, [e]dit, [n]o: ")? {
            'y' => {
                fs::remove_file(&path).ok();
                return Ok(Some(edited));
            }
            'e' => continue,
            _ => return kept("Not sent"),
        }
    }
}

/// Runs `$VISUAL` (or `$EDITOR`, else vi) on `path`.
pub fn editor(path: &Path) -> Result<()> {
    let editor = ["VISUAL", "EDITOR"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .find(|e| !e.trim().is_empty())
        .unwrap_or_else(|| "vi".into());
    let status = Command::new("sh")
        .args(["-c", &format!("{editor} \"$1\""), "sh"])
        .arg(path)
        .status()
        .with_context(|| format!("running {editor}"))?;
    if !status.success() {
        bail!("{editor} failed");
    }
    Ok(())
}

/// The first letter of the answer to `question`, lower case.
fn ask(question: &str) -> Result<char> {
    eprint!("{question}");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(answer
        .trim()
        .chars()
        .next()
        .unwrap_or('n')
        .to_ascii_lowercase())
}

fn read_stdin() -> Result<String> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .context("reading the message from stdin")?;
    Ok(text)
}

/// Sends `draft` over SMTP, then puts a copy in the sent mailbox.
pub async fn send(account: &Account, from: &Mailbox, draft: &Draft) -> Result<()> {
    // graviola is pure Rust, see `kube::cloudflare::client`.
    let _ = rustls_graviola::default_provider().install_default();
    let raw = draft.build(from)?;
    let password = Secrets::default().password(account)?;
    let host = account.smtp_host();
    let message = draft.recipients().into_iter().fold(
        mail_send::smtp::message::Message::empty()
            .from(from.address.as_str())
            .body(raw.as_slice()),
        |message, rcpt| message.to(rcpt),
    );
    let mut client = SmtpClientBuilder::new(host.as_str(), account.smtp_port)
        .map_err(anyhow::Error::msg)?
        .implicit_tls(account.smtp_port == 465)
        .credentials((account.user.as_str(), password.as_str()))
        .timeout(std::time::Duration::from_secs(60))
        .connect()
        .await
        .with_context(|| {
            format!(
                "connecting to {host}:{} as {}",
                account.smtp_port, account.user
            )
        })?;
    client
        .send(message)
        .await
        .with_context(|| format!("sending through {host}"))?;
    let _ = client.quit().await;
    println!("Sent to {}", draft.recipients().join(", "));
    if let Some(mailbox) = &account.sent_mailbox {
        let saved = async {
            let mut session = inbox::login(account, &password).await?;
            session
                .append(mailbox, Some("(\\Seen)"), None, &raw)
                .await?;
            let _ = session.logout().await;
            anyhow::Ok(())
        };
        if let Err(err) = saved.await {
            eprintln!("uc mail: sent, but no copy in {mailbox}: {err:#}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ada() -> Mailbox {
        Mailbox {
            name: Some("Ada Lovelace".into()),
            address: "ada@example.com".into(),
        }
    }

    #[test]
    fn addresses_parse() {
        let parsed = parse_addresses("Ada Lovelace <ada@example.com>, bob@example.com").unwrap();
        assert_eq!(parsed[0], ada());
        assert_eq!(parsed[1].address, "bob@example.com");
        assert_eq!(parsed[1].name, None);
        assert!(parse_addresses("not an address").is_err());
        assert!(parse_addresses("  ").unwrap().is_empty());
    }

    #[test]
    fn draft_text_round_trips() {
        let draft = Draft {
            to: vec![ada()],
            cc: parse_addresses("\"Smith, Bob\" <bob@example.com>").unwrap(),
            subject: "Hello: there".into(),
            body: "Hi\n\nBye\n".into(),
            attachments: vec!["/tmp/a.pdf".into()],
            in_reply_to: Some("x@y".into()),
            ..Draft::default()
        };
        let text = draft.text(&ada(), "work");
        assert_eq!(draft.read_text(&text).unwrap(), draft);
    }

    #[test]
    fn reply_all_leaves_me_out() {
        let raw = b"From: Bob <bob@example.com>\r\nTo: ada@example.com, eve@example.com\r\n\
Cc: Bob <bob@example.com>, joe@example.com\r\nSubject: Plans\r\n\
Message-ID: <2@example.com>\r\nReferences: <1@example.com>\r\n\r\nHi\r\n\r\nall\r\n";
        let one = Draft::reply_to(raw, &ada(), false).unwrap();
        assert_eq!(one.to, parse_addresses("Bob <bob@example.com>").unwrap());
        assert!(one.cc.is_empty());
        assert_eq!(one.subject, "Re: Plans");
        assert_eq!(one.in_reply_to.as_deref(), Some("2@example.com"));
        assert_eq!(one.references, ["1@example.com", "2@example.com"]);
        assert!(one.body.ends_with("wrote:\n> Hi\n>\n> all\n"));
        let all = Draft::reply_to(raw, &ada(), true).unwrap();
        assert_eq!(
            all.cc,
            parse_addresses("eve@example.com, joe@example.com").unwrap()
        );
        let mine = b"From: ada@example.com\r\nTo: bob@example.com\r\nSubject: Re: x\r\n\r\nHi\r\n";
        let mine = Draft::reply_to(mine, &ada(), false).unwrap();
        assert_eq!(mine.to, parse_addresses("bob@example.com").unwrap());
        assert_eq!(mine.subject, "Re: x");
    }

    #[test]
    fn unknown_headers_are_refused() {
        assert!(Draft::default().read_text("Too: x@y\n\nhi").is_err());
    }

    #[test]
    fn bcc_is_sent_to_but_not_shown() {
        let draft = Draft {
            to: vec![ada()],
            bcc: parse_addresses("eve@example.com, ADA@example.com").unwrap(),
            subject: "s".into(),
            body: "b".into(),
            ..Draft::default()
        };
        assert_eq!(draft.recipients(), ["ada@example.com", "eve@example.com"]);
        let raw = String::from_utf8(draft.build(&ada()).unwrap()).unwrap();
        assert!(!raw.contains("eve@"));
        assert!(raw.contains("To: \"Ada Lovelace\" <ada@example.com>"));
    }
}
