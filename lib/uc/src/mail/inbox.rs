// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc mail pull` and `uc mail view`: your mail, kept on this machine.
//! [`super::compose`] sends it.
//!
//! The accounts are in [`config_path`]. [`pull`] logs in to each over IMAPS
//! and fetches what came since its last pull into [`data_dir`]: for every
//! account and mailbox a directory with one `<uidvalidity>-<uid>.eml` per
//! message, an `index.jsonl` of who sent them, their subjects and when the
//! server got them, and a `state.json` with the last UID fetched. [`entries`]
//! reads the indexes back, the newest mail first.

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use mail_parser::{Address, MessageParser, MimeHeaders};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tokio_rustls::rustls::ClientConfig;
use tokio_rustls::rustls::pki_types::ServerName;

/// How many messages one `UID FETCH` asks for.
const BATCH: usize = 50;

/// `~/.config/usecode/mail.yaml`, or `UC_MAIL_CONFIG`.
pub fn config_path() -> PathBuf {
    match std::env::var_os("UC_MAIL_CONFIG").filter(|p| !p.is_empty()) {
        Some(path) => PathBuf::from(path),
        None => home().join(".config/usecode/mail.yaml"),
    }
}

/// `UC_MAIL_DIR`, else `$XDG_DATA_HOME/usecode/mail`, else
/// `~/.local/share/usecode/mail`.
pub fn data_dir() -> PathBuf {
    let env = |name| std::env::var_os(name).filter(|p| !p.is_empty());
    match (env("UC_MAIL_DIR"), env("XDG_DATA_HOME")) {
        (Some(dir), _) => PathBuf::from(dir),
        (None, Some(data)) => PathBuf::from(data).join("usecode/mail"),
        (None, None) => home().join(".local/share/usecode/mail"),
    }
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub accounts: Vec<Account>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    /// What `--account` and the mail's ids call it, e.g. `work`.
    pub name: String,
    /// The IMAP server, e.g. `imap.gmail.com`.
    pub host: String,
    #[serde(default = "imaps_port")]
    pub port: u16,
    pub user: String,
    /// Where the password is in a `uc secret` store, e.g. `mail.work`.
    pub password_secret: Option<String>,
    /// That store [default: the unnamed one].
    #[serde(default)]
    pub secret_store: String,
    #[serde(default = "inbox")]
    pub mailboxes: Vec<String>,
    /// Who mail goes out from, e.g. `Ada Lovelace <ada@example.com>`
    /// [default: user].
    pub from: Option<String>,
    /// The SMTP server [default: host, with `imap.` made `smtp.`].
    pub smtp_host: Option<String>,
    /// 465 is TLS from the start, any other port STARTTLS.
    #[serde(default = "submissions_port")]
    pub smtp_port: u16,
    /// The mailbox a copy of sent mail goes to, e.g. `Sent`. Leave it out
    /// when the server keeps one itself, like Gmail does.
    pub sent_mailbox: Option<String>,
}

impl Account {
    /// [`Account::smtp_host`] or its default.
    pub fn smtp_host(&self) -> String {
        match (&self.smtp_host, self.host.strip_prefix("imap.")) {
            (Some(host), _) => host.clone(),
            (None, Some(domain)) => format!("smtp.{domain}"),
            (None, None) => self.host.clone(),
        }
    }
}

fn imaps_port() -> u16 {
    993
}

fn submissions_port() -> u16 {
    465
}

fn inbox() -> Vec<String> {
    vec!["INBOX".into()]
}

const EXAMPLE_CONFIG: &str = "\
accounts:
  - name: work
    host: imap.example.com
    user: ada@example.com
    password_secret: mail.work   # uc secret edit, or UC_MAIL_PASSWORD_WORK
    # port: 993
    # mailboxes: [INBOX, Archive]
    # from: Ada Lovelace <ada@example.com>
    # smtp_host: smtp.example.com
    # smtp_port: 465                 # 587 for STARTTLS
    # sent_mailbox: Sent";

impl Config {
    pub fn load() -> Result<Config> {
        let path = config_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => bail!(
                "no accounts yet: put them in {}, like\n\n{EXAMPLE_CONFIG}",
                path.display()
            ),
            Err(err) => return Err(err).with_context(|| format!("reading {}", path.display())),
        };
        let config: Config =
            serde_yaml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        for account in &config.accounts {
            if account.name.is_empty() || dir_name(&account.name) != account.name {
                bail!(
                    "{}: account name {:?} may only have letters, digits and -_.@+",
                    path.display(),
                    account.name
                );
            }
        }
        Ok(config)
    }

    /// The accounts called `names`, or all of them when there are none.
    pub fn select(&self, names: &[String]) -> Result<Vec<&Account>> {
        if let Some(name) = names
            .iter()
            .find(|name| !self.accounts.iter().any(|a| &a.name == *name))
        {
            bail!("no account {name} in {}", config_path().display());
        }
        Ok(self
            .accounts
            .iter()
            .filter(|a| names.is_empty() || names.contains(&a.name))
            .collect())
    }
}

/// A file or directory name for `name`: anything but letters, digits and
/// `-_.@+` becomes `_`, so `[Gmail]/All Mail` is `_Gmail__All_Mail`.
fn dir_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            c if c.is_alphanumeric() || "-_.@+".contains(c) => c,
            _ => '_',
        })
        .collect()
}

/// One message on this machine, a line of an `index.jsonl`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    /// `<account>/<mailbox>/<uidvalidity>-<uid>`: its file in [`data_dir`],
    /// without the `.eml`.
    pub id: String,
    pub account: String,
    pub mailbox: String,
    /// When the server got it, in seconds since the epoch.
    pub received: i64,
    pub from: String,
    pub subject: String,
}

impl Entry {
    /// The received time in the local time zone, e.g. `2026-10-22 14:03`.
    pub fn date(&self) -> String {
        chrono::DateTime::from_timestamp(self.received, 0)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d %H:%M")
                    .to_string()
            })
            .unwrap_or_default()
    }
}

#[derive(Serialize, Deserialize, Default)]
struct State {
    uid_validity: u32,
    last_uid: u32,
}

/// Fetch the new mail of the accounts called `names` (all when empty). With
/// `last`, a mailbox only gets its newest `last` new messages, and the older
/// ones are skipped for good.
pub async fn pull(names: &[String], last: Option<usize>) -> Result<()> {
    // graviola is pure Rust, see `kube::cloudflare::client`.
    let _ = rustls_graviola::default_provider().install_default();
    let config = Config::load()?;
    let mut secrets = Secrets::default();
    let mut failed = 0;
    for account in config.select(names)? {
        let result = match secrets.password(account) {
            Ok(password) => pull_account(account, &password, last).await,
            Err(err) => Err(err),
        };
        if let Err(err) = result {
            eprintln!("uc mail pull: {}: {err:#}", account.name);
            failed += 1;
        }
    }
    match failed {
        0 => Ok(()),
        n => bail!("{n} account(s) could not be pulled"),
    }
}

/// The `uc secret` stores, each decrypted once.
#[derive(Default)]
pub(crate) struct Secrets(HashMap<String, serde_json::Value>);

impl Secrets {
    /// `UC_MAIL_PASSWORD_<NAME>`, else the account's `password_secret`, else
    /// asked for on the terminal.
    pub(crate) fn password(&mut self, account: &Account) -> Result<zeroize::Zeroizing<String>> {
        let env = format!(
            "UC_MAIL_PASSWORD_{}",
            account
                .name
                .to_ascii_uppercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "_")
        );
        if let Some(password) = std::env::var(&env).ok().filter(|p| !p.is_empty()) {
            return Ok(password.into());
        }
        if let Some(path) = &account.password_secret {
            let store = &account.secret_store;
            if !self.0.contains_key(store) {
                let secrets = crate::secret::Store::new(&crate::secret::dir(), store)
                    .get()
                    .context("reading the secrets")?;
                self.0.insert(store.clone(), secrets);
            }
            return crate::secret::field(&self.0[store], path)
                .filter(|value| value.is_string())
                .map(|value| crate::secret::raw(value).into())
                .with_context(|| format!("no {path} in the secrets (or set {env})"));
        }
        if !std::io::stdin().is_terminal() {
            bail!(
                "no password: set password_secret in {} or {env}",
                config_path().display()
            );
        }
        let prompt = format!("Password for {} ({}): ", account.user, account.name);
        Ok(rpassword::prompt_password(prompt)?.into())
    }
}

async fn pull_account(account: &Account, password: &str, last: Option<usize>) -> Result<()> {
    let mut session = login(account, password).await?;
    for mailbox in &account.mailboxes {
        let new = pull_mailbox(&mut session, account, mailbox, last)
            .await
            .with_context(|| format!("pulling {mailbox}"))?;
        println!("{} {mailbox}: {new} new", account.name);
    }
    let _ = session.logout().await;
    Ok(())
}

/// An IMAP session of `account`, logged in over TLS.
pub(crate) async fn login(
    account: &Account,
    password: &str,
) -> Result<async_imap::Session<tokio_rustls::client::TlsStream<TcpStream>>> {
    let host = account.host.as_str();
    let tcp = TcpStream::connect((host, account.port))
        .await
        .with_context(|| format!("connecting to {host}:{}", account.port))?;
    let tls = {
        use rustls_platform_verifier::ConfigVerifierExt;
        let config = ClientConfig::with_platform_verifier()?;
        let name = ServerName::try_from(host.to_string())?;
        TlsConnector::from(Arc::new(config))
            .connect(name, tcp)
            .await
            .with_context(|| format!("TLS with {host}"))?
    };
    let mut client = async_imap::Client::new(tls);
    client
        .read_response()
        .await?
        .context("the server hung up before greeting")?;
    client
        .login(&account.user, password)
        .await
        .map_err(|(err, _)| err)
        .with_context(|| format!("logging in to {host} as {}", account.user))
}

async fn pull_mailbox<T>(
    session: &mut async_imap::Session<T>,
    account: &Account,
    mailbox: &str,
    last: Option<usize>,
) -> Result<usize>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    let selected = session.examine(mailbox).await?;
    let validity = selected.uid_validity.unwrap_or(0);
    let mailbox_dir = dir_name(mailbox);
    let dir = data_dir().join(&account.name).join(&mailbox_dir);
    fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    let state_file = dir.join("state.json");
    let mut state = match fs::read(&state_file) {
        Ok(json) => serde_json::from_slice(&json)
            .with_context(|| format!("reading {}", state_file.display()))?,
        Err(_) => State::default(),
    };
    if state.uid_validity != validity {
        // The server numbered the mailbox anew, so its UIDs start over.
        state = State {
            uid_validity: validity,
            last_uid: 0,
        };
    }

    // `n:*` always matches the newest message, even below n.
    let mut uids: Vec<u32> = session
        .uid_search(format!("UID {}:*", state.last_uid + 1))
        .await?
        .into_iter()
        .filter(|&uid| uid > state.last_uid)
        .collect();
    uids.sort_unstable();
    if let Some(last) = last {
        uids.drain(..uids.len().saturating_sub(last));
    }

    let index_file = dir.join("index.jsonl");
    let mut index = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_file)
        .with_context(|| format!("opening {}", index_file.display()))?;
    let mut new = 0;
    for batch in uids.chunks(BATCH) {
        let set = batch
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut fetches = session
            .uid_fetch(set, "(UID INTERNALDATE BODY.PEEK[])")
            .await?;
        while let Some(fetch) = fetches.next().await {
            let fetch = fetch?;
            let (Some(uid), Some(raw)) = (fetch.uid, fetch.body()) else {
                continue;
            };
            let name = format!("{validity}-{uid}");
            let file = dir.join(format!("{name}.eml"));
            let part = dir.join(format!(".{name}.eml.part"));
            fs::write(&part, raw).with_context(|| format!("writing {}", part.display()))?;
            fs::rename(&part, &file)?;
            let entry = index_entry(
                format!("{}/{mailbox_dir}/{name}", account.name),
                account,
                mailbox,
                fetch.internal_date().map(|t| t.timestamp()),
                raw,
            );
            writeln!(index, "{}", serde_json::to_string(&entry)?)?;
            state.last_uid = state.last_uid.max(uid);
            new += 1;
        }
        drop(fetches);
        // Past the ones deleted since the search too.
        state.last_uid = state.last_uid.max(batch.iter().copied().max().unwrap_or(0));
        fs::write(&state_file, serde_json::to_vec(&state)?)?;
    }
    Ok(new)
}

fn index_entry(
    id: String,
    account: &Account,
    mailbox: &str,
    received: Option<i64>,
    raw: &[u8],
) -> Entry {
    let message = MessageParser::default().parse(raw);
    let header = message.as_ref();
    let received = received
        .or_else(|| header.and_then(|m| m.date()).map(|d| d.to_timestamp()))
        .unwrap_or_else(|| chrono::Utc::now().timestamp());
    Entry {
        id,
        account: account.name.clone(),
        mailbox: mailbox.to_string(),
        received,
        from: header
            .and_then(|m| m.from())
            .and_then(|from| from.first())
            .map(|addr| match (addr.name(), addr.address()) {
                (Some(name), _) if !name.trim().is_empty() => name.trim().to_string(),
                (_, Some(address)) => address.to_string(),
                _ => String::new(),
            })
            .unwrap_or_default(),
        subject: header
            .and_then(|m| m.subject())
            .map(|s| s.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default(),
    }
}

/// The mail on this machine, of the account `account` when given, the
/// newest first.
pub fn entries(account: Option<&str>) -> Result<Vec<Entry>> {
    let root = data_dir();
    let mut entries: HashMap<String, Entry> = HashMap::new();
    for account_dir in subdirs(&root)? {
        let name = account_dir
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if account.is_some_and(|account| account != name) {
            continue;
        }
        for mailbox_dir in subdirs(&account_dir)? {
            let Ok(index) = fs::read_to_string(mailbox_dir.join("index.jsonl")) else {
                continue;
            };
            // A pull cut short may have indexed a message twice.
            for entry in index
                .lines()
                .filter_map(|l| serde_json::from_str::<Entry>(l).ok())
            {
                entries.insert(entry.id.clone(), entry);
            }
        }
    }
    let mut entries: Vec<Entry> = entries.into_values().collect();
    sort(&mut entries);
    Ok(entries)
}

/// The newest first.
fn sort(entries: &mut [Entry]) {
    entries.sort_by(|a, b| b.received.cmp(&a.received).then_with(|| b.id.cmp(&a.id)));
}

fn subdirs(dir: &Path) -> Result<Vec<PathBuf>> {
    let read = match fs::read_dir(dir) {
        Ok(read) => read,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err).with_context(|| format!("reading {}", dir.display())),
    };
    Ok(read
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect())
}

/// The message `id` as it came from the server.
pub fn raw(id: &str) -> Result<Vec<u8>> {
    if id.split('/').count() != 3
        || id
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.'))
    {
        bail!("{id} is not a mail id, like work/INBOX/1-42");
    }
    let path = data_dir().join(format!("{id}.eml"));
    fs::read(&path).with_context(|| format!("reading {}", path.display()))
}

/// The message `id` for reading: its headers, its text, and the names of
/// its attachments.
pub fn render(id: &str) -> Result<String> {
    let raw = raw(id)?;
    let Some(message) = MessageParser::default().parse(&raw) else {
        return Ok(String::from_utf8_lossy(&raw).into_owned());
    };
    let mut out = format!("Id:      {id}\n");
    if let Some(date) = message.date() {
        out += &format!("Date:    {}\n", date.to_rfc822());
    }
    for (name, address) in [
        ("From:   ", message.from()),
        ("To:     ", message.to()),
        ("Cc:     ", message.cc()),
    ] {
        if let Some(address) = address {
            out += &format!("{name} {}\n", addresses(address));
        }
    }
    out += &format!("Subject: {}\n\n", message.subject().unwrap_or_default());
    out += message
        .body_text(0)
        .as_deref()
        .unwrap_or("(no text)")
        .trim_end();
    out += "\n";
    for attachment in message.attachments() {
        out += &format!(
            "\n[attachment: {} ({} bytes)]",
            attachment.attachment_name().unwrap_or("unnamed"),
            attachment.len()
        );
    }
    Ok(out)
}

fn addresses(address: &Address) -> String {
    address
        .iter()
        .map(|addr| match (addr.name(), addr.address()) {
            (Some(name), Some(address)) => format!("{name} <{address}>"),
            (None, Some(address)) => address.to_string(),
            (Some(name), None) => name.to_string(),
            (None, None) => String::new(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Pick messages with skim, reading each in the preview, and return the
/// picked ones' ids.
pub fn pick(entries: Vec<Entry>) -> Vec<String> {
    use skim::prelude::*;

    struct Item(Entry, String);

    impl SkimItem for Item {
        fn text(&self) -> Cow<'_, str> {
            Cow::Borrowed(&self.1)
        }

        fn preview(&self, _context: PreviewContext) -> ItemPreview {
            ItemPreview::Text(render(&self.0.id).unwrap_or_else(|err| format!("{err:#}")))
        }

        fn output(&self) -> Cow<'_, str> {
            Cow::Borrowed(&self.0.id)
        }
    }

    let options = SkimOptionsBuilder::default()
        .multi(true)
        .reverse(true)
        .tiebreak(vec![RankCriteria::Score, RankCriteria::Index])
        .preview(Some(String::new()))
        .preview_window("down:60%:wrap".to_string())
        .prompt("mail> ".to_string())
        .build()
        .expect("skim options");
    let (tx, rx): (SkimItemSender, SkimItemReceiver) = unbounded();
    for entry in entries {
        let line = line(&entry);
        let _ = tx.send(Arc::new(Item(entry, line)));
    }
    drop(tx);
    match Skim::run_with(&options, Some(rx)) {
        Some(out) if !out.is_abort => out
            .selected_items
            .iter()
            .map(|item| item.output().into_owned())
            .collect(),
        _ => Vec::new(),
    }
}

/// One line about a message: when it came, the account, who sent it and
/// what about.
pub fn line(entry: &Entry) -> String {
    let from: String = entry.from.chars().take(24).collect();
    format!(
        "{}  {}  {from:<24}  {}",
        entry.date(),
        entry.account,
        entry.subject
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, received: i64) -> Entry {
        Entry {
            id: id.into(),
            account: "work".into(),
            mailbox: "INBOX".into(),
            received,
            from: String::new(),
            subject: String::new(),
        }
    }

    #[test]
    fn mailbox_names_become_safe_dirs() {
        assert_eq!(dir_name("[Gmail]/All Mail"), "_Gmail__All_Mail");
        assert_eq!(dir_name("INBOX"), "INBOX");
        assert_eq!(dir_name("../x"), ".._x");
    }

    #[test]
    fn newest_first() {
        let mut entries = vec![entry("a", 1), entry("c", 3), entry("b", 2)];
        sort(&mut entries);
        let ids: Vec<_> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["c", "b", "a"]);
    }

    #[test]
    fn ids_stay_inside_the_mail_dir() {
        assert!(raw("../../etc/passwd").is_err());
        assert!(raw("work/../x").is_err());
        assert!(raw("work/INBOX").is_err());
    }

    #[test]
    fn index_entry_takes_sender_and_subject() {
        let account = Account {
            name: "work".into(),
            host: "imap.example.com".into(),
            port: 993,
            user: "ada@example.com".into(),
            password_secret: None,
            secret_store: String::new(),
            mailboxes: inbox(),
            from: None,
            smtp_host: None,
            smtp_port: submissions_port(),
            sent_mailbox: None,
        };
        let raw = b"From: Ada Lovelace <ada@example.com>\r\nSubject: Hello\r\n  there\r\nDate: Thu, 22 Oct 2026 10:00:00 +0000\r\n\r\nHi\r\n";
        let e = index_entry("work/INBOX/1-1".into(), &account, "INBOX", None, raw);
        assert_eq!(e.from, "Ada Lovelace");
        assert_eq!(e.subject, "Hello there");
        assert_eq!(e.received, 1792663200);
    }
}
