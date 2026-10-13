// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc mail domain` and `uc mail account`: the domains Stalwart takes mail
//! for and the accounts on them, through [`Stalwart`]'s management API.
//!
//! Stalwart's lists come as objects keyed `"0"`, `"1"`, ... and its sets
//! as objects of `true`s; a list is written back whole.

use super::configure::confirm;
use super::stalwart::Stalwart;
use crate::password::{self, Charset};
use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value, json};
use std::io::{BufRead, IsTerminal};
use zeroize::Zeroizing;

/// The domains Stalwart has, to go between names and ids.
struct Domains(Vec<Value>);

impl Domains {
    async fn load(stalwart: &Stalwart) -> Result<Domains> {
        Ok(Domains(stalwart.get("x:Domain").await?))
    }

    fn find(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|d| d["name"] == name)
    }

    fn id(&self, name: &str) -> Result<String> {
        self.find(name)
            .and_then(|d| d["id"].as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Stalwart has no domain {name}: uc mail domain add {name}"))
    }

    fn name<'a>(&'a self, id: &Value) -> &'a str {
        self.0
            .iter()
            .find(|d| d["id"] == *id)
            .and_then(|d| d["name"].as_str())
            .unwrap_or("?")
    }
}

/// `ada@example.com` as `("ada", "example.com")`.
pub fn address(text: &str) -> Result<(String, String)> {
    let text = text.trim().to_lowercase();
    match text.split_once('@') {
        Some((local, domain))
            if !local.is_empty() && domain.contains('.') && !domain.contains('@') =>
        {
            Ok((local.to_string(), domain.trim_end_matches('.').to_string()))
        }
        _ => bail!("{text} is not an address like ada@example.com"),
    }
}

/// A domain name as Stalwart keeps it: lowercase, without the final dot.
fn domain_name(text: &str) -> Result<String> {
    let name = text.trim().trim_end_matches('.').to_lowercase();
    if !name.contains('.') || name.contains('@') || name.contains(char::is_whitespace) {
        bail!("{text} is not a domain like example.com");
    }
    Ok(name)
}

/// The items of a Stalwart list, in order.
fn items(list: &Value) -> Vec<Value> {
    match list {
        Value::Array(a) => a.clone(),
        Value::Object(o) => {
            let mut keyed: Vec<(&String, &Value)> = o.iter().collect();
            keyed.sort_by_key(|(k, _)| k.parse::<usize>().unwrap_or(usize::MAX));
            keyed.into_iter().map(|(_, v)| v.clone()).collect()
        }
        _ => Vec::new(),
    }
}

/// `items` as a Stalwart list.
fn list(items: Vec<Value>) -> Value {
    Value::Object(
        items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v))
            .collect::<Map<_, _>>(),
    )
}

/// `5G`, `500M`, `1T` or a number of bytes; K, M, G and T are powers of
/// 1024.
pub fn size(text: &str) -> Result<u64> {
    let t = text.trim().to_uppercase();
    let t = t.trim_end_matches("IB").trim_end_matches('B');
    let (digits, unit) = match t.find(|c: char| !c.is_ascii_digit()) {
        Some(i) => t.split_at(i),
        None => (t, ""),
    };
    let shift = match unit {
        "" => 0,
        "K" => 10,
        "M" => 20,
        "G" => 30,
        "T" => 40,
        _ => bail!("{text} is not a size like 5G or 500M"),
    };
    let n: u64 = digits
        .parse()
        .map_err(|_| anyhow!("{text} is not a size like 5G or 500M"))?;
    n.checked_mul(1 << shift)
        .ok_or_else(|| anyhow!("{text} is too big"))
}

fn human(bytes: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 || value >= 10.0 || value.fract() == 0.0 {
        format!("{value:.0} {}", units[unit])
    } else {
        format!("{value:.1} {}", units[unit])
    }
}

/// `uc mail domain list`.
pub async fn list_domains(context: &str) -> Result<()> {
    let stalwart = Stalwart::admin(context).await?;
    let domains = stalwart.get("x:Domain").await?;
    let accounts = stalwart.get("x:Account").await?;
    if domains.is_empty() {
        println!("no domains yet: uc mail domain add example.com");
        return Ok(());
    }
    let width = domains
        .iter()
        .filter_map(|d| d["name"].as_str())
        .map(str::len)
        .max()
        .unwrap_or(0);
    let mut rows: Vec<(&str, String)> = domains
        .iter()
        .map(|d| {
            let n = accounts.iter().filter(|a| a["domainId"] == d["id"]).count();
            let mut notes = vec![format!("{n} account{}", if n == 1 { "" } else { "s" })];
            let aliases: Vec<&str> = d["aliases"]
                .as_object()
                .map(|a| a.keys().map(String::as_str).collect())
                .unwrap_or_default();
            if !aliases.is_empty() {
                notes.push(format!("also {}", aliases.join(", ")));
            }
            if let Some(to) = d["catchAllAddress"].as_str() {
                notes.push(format!("catch-all to {to}"));
            }
            if d["isEnabled"] == false {
                notes.push("disabled".into());
            }
            (d["name"].as_str().unwrap_or("?"), notes.join(", "))
        })
        .collect();
    rows.sort();
    for (name, notes) in rows {
        println!("{name:width$}  {notes}");
    }
    Ok(())
}

/// `uc mail domain add`: a domain, with Stalwart making its DKIM keys.
pub async fn add_domain(context: &str, name: &str, aliases: &[String]) -> Result<()> {
    let name = domain_name(name)?;
    let aliases: Map<String, Value> = aliases
        .iter()
        .map(|a| Ok((domain_name(a)?, json!(true))))
        .collect::<Result<_>>()?;
    let stalwart = Stalwart::admin(context).await?;
    let domains = Domains::load(&stalwart).await?;
    if domains.find(&name).is_some() {
        bail!("Stalwart has {name} already");
    }
    stalwart
        .create(
            "x:Domain",
            json!({
                "name": name,
                "aliases": aliases,
                "certificateManagement": {"@type": "Manual"},
                "dkimManagement": {"@type": "Automatic"},
                "dnsManagement": {"@type": "Manual"},
                "subAddressing": {"@type": "Enabled"},
            }),
        )
        .await?;
    println!("added {name}");
    println!("\nNext:");
    println!("  uc mail configure {name}        its DNS records: MX, SPF, DKIM, DMARC, ...");
    println!("  uc mail account add you@{name}  an account to read its mail with");
    Ok(())
}

/// `uc mail domain rm`: a domain without accounts.
pub async fn remove_domain(context: &str, name: &str, yes: bool) -> Result<()> {
    let name = domain_name(name)?;
    let stalwart = Stalwart::admin(context).await?;
    let domains = Domains::load(&stalwart).await?;
    let id = domains.id(&name)?;
    let accounts: Vec<String> = stalwart
        .get("x:Account")
        .await?
        .iter()
        .filter(|a| a["domainId"] == id.as_str())
        .filter_map(|a| a["emailAddress"].as_str().map(str::to_string))
        .collect();
    if !accounts.is_empty() {
        bail!(
            "{name} still has {}; remove them first (uc mail account rm)",
            accounts.join(", ")
        );
    }
    if !yes {
        confirm(&format!("Delete the domain {name} from Stalwart?"))?;
    }
    stalwart.destroy("x:Domain", &id).await?;
    println!("deleted {name}; its DNS records are still there, remove them where they're kept");
    Ok(())
}

/// `uc mail domain catch-all`: where mail to unknown addresses of `name`
/// goes; shows it without `to` or `off`.
pub async fn catch_all(context: &str, name: &str, to: Option<&str>, off: bool) -> Result<()> {
    let name = domain_name(name)?;
    let stalwart = Stalwart::admin(context).await?;
    let domains = Domains::load(&stalwart).await?;
    let id = domains.id(&name)?;
    let value = match (to, off) {
        (Some(_), true) => bail!("either an address or --off"),
        (None, false) => {
            match domains
                .find(&name)
                .and_then(|d| d["catchAllAddress"].as_str())
            {
                Some(to) => println!("mail to unknown addresses of {name} goes to {to}"),
                None => println!("mail to unknown addresses of {name} is refused"),
            }
            return Ok(());
        }
        (None, true) => Value::Null,
        (Some(to), false) => {
            let (local, domain) = address(to)?;
            json!(format!("{local}@{domain}"))
        }
    };
    stalwart
        .update("x:Domain", &id, json!({"catchAllAddress": value}))
        .await?;
    match value.as_str() {
        Some(to) => println!("mail to unknown addresses of {name} now goes to {to}"),
        None => println!("mail to unknown addresses of {name} is now refused"),
    }
    Ok(())
}

/// The accounts and their domains.
struct Accounts {
    stalwart: Stalwart,
    domains: Domains,
    all: Vec<Value>,
}

impl Accounts {
    async fn load(context: &str) -> Result<Accounts> {
        let stalwart = Stalwart::admin(context).await?;
        let domains = Domains::load(&stalwart).await?;
        let all = stalwart.get("x:Account").await?;
        Ok(Accounts {
            stalwart,
            domains,
            all,
        })
    }

    fn email(&self, a: &Value) -> String {
        match a["emailAddress"].as_str() {
            Some(e) => e.to_string(),
            None => format!(
                "{}@{}",
                a["name"].as_str().unwrap_or("?"),
                self.domains.name(&a["domainId"])
            ),
        }
    }

    fn aliases(&self, a: &Value) -> Vec<String> {
        items(&a["aliases"])
            .iter()
            .map(|x| {
                format!(
                    "{}@{}",
                    x["name"].as_str().unwrap_or("?"),
                    self.domains.name(&x["domainId"])
                )
            })
            .collect()
    }

    /// The account with this address.
    fn find(&self, text: &str) -> Result<&Value> {
        let (local, domain) = address(text)?;
        let addr = format!("{local}@{domain}");
        if let Some(a) = self.all.iter().find(|a| self.email(a) == addr) {
            return Ok(a);
        }
        if let Some(a) = self.all.iter().find(|a| self.aliases(a).contains(&addr)) {
            bail!("{addr} is an alias of {}", self.email(a));
        }
        bail!("there's no account {addr}: uc mail account list")
    }

    /// Who has `addr`, as an address or an alias.
    fn owner(&self, addr: &str) -> Option<String> {
        self.all
            .iter()
            .find(|a| self.email(a) == addr || self.aliases(a).iter().any(|x| x == addr))
            .map(|a| self.email(a))
    }
}

fn id(account: &Value) -> Result<String> {
    account["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("an account without an id: {account}"))
}

/// `uc mail account list`, of one domain or all.
pub async fn list_accounts(context: &str, domain: Option<&str>) -> Result<()> {
    let accounts = Accounts::load(context).await?;
    let only = domain.map(domain_name).transpose()?;
    if let Some(d) = &only {
        accounts.domains.id(d)?;
    }
    let mut rows: Vec<(String, String)> = accounts
        .all
        .iter()
        .filter(|a| {
            only.as_deref()
                .is_none_or(|d| accounts.domains.name(&a["domainId"]) == d)
        })
        .map(|a| {
            let mut notes = Vec::new();
            if a["@type"] == "Group" {
                notes.push("group".to_string());
            }
            if a["roles"]["@type"] == "Admin" {
                notes.push("admin".into());
            }
            let used = a["usedDiskQuota"].as_u64().unwrap_or(0);
            notes.push(match a["quotas"]["maxDiskQuota"].as_u64() {
                Some(max) if max > 0 => format!("{} of {}", human(used), human(max)),
                _ => human(used),
            });
            let aliases = accounts.aliases(a);
            if !aliases.is_empty() {
                notes.push(format!("also {}", aliases.join(", ")));
            }
            if let Some(d) = a["description"].as_str().filter(|d| !d.is_empty()) {
                notes.push(d.to_string());
            }
            (accounts.email(a), notes.join(", "))
        })
        .collect();
    if rows.is_empty() {
        let at = only.as_deref().unwrap_or("example.com");
        println!("no accounts yet: uc mail account add you@{at}");
        return Ok(());
    }
    rows.sort();
    let width = rows.iter().map(|(e, _)| e.len()).max().unwrap_or(0);
    for (email, notes) in rows {
        println!("{email:width$}  {notes}");
    }
    Ok(())
}

/// What a new or changed account is made with.
pub struct NewAccount {
    pub admin: bool,
    pub description: Option<String>,
    pub quota: Option<u64>,
    pub random: bool,
}

/// `uc mail account add`.
pub async fn add_account(context: &str, addr: &str, new: NewAccount) -> Result<()> {
    let (local, domain) = address(addr)?;
    let addr = format!("{local}@{domain}");
    let accounts = Accounts::load(context).await?;
    let domain_id = accounts.domains.id(&domain)?;
    if let Some(owner) = accounts.owner(&addr) {
        bail!("{owner} has {addr} already");
    }
    let secret = password(new.random)?;
    let mut object = json!({
        "@type": "User",
        "name": local,
        "domainId": domain_id,
        "credentials": list(vec![json!({"@type": "Password", "secret": secret.as_str()})]),
        "roles": {"@type": if new.admin { "Admin" } else { "User" }},
        "permissions": {"@type": "Inherit"},
        "encryptionAtRest": {"@type": "Disabled"},
        "aliases": {},
        "memberGroupIds": {},
        "quotas": {},
    });
    if let Some(q) = new.quota {
        object["quotas"] = json!({"maxDiskQuota": q});
    }
    if let Some(d) = new.description {
        object["description"] = json!(d);
    }
    accounts.stalwart.create("x:Account", object).await?;
    println!("added {addr}");
    if new.random {
        println!("its password: {}", secret.as_str());
    }
    println!("\nWhat to type into a mail app: uc mail settings {addr}");
    Ok(())
}

/// `uc mail account rm`: the account and all its mail.
pub async fn remove_account(context: &str, addr: &str, yes: bool) -> Result<()> {
    let accounts = Accounts::load(context).await?;
    let account = accounts.find(addr)?;
    let email = accounts.email(account);
    if !yes {
        confirm(&format!("Delete {email} and all its mail?"))?;
    }
    accounts
        .stalwart
        .destroy("x:Account", &id(account)?)
        .await?;
    println!("deleted {email}");
    Ok(())
}

/// `uc mail account password`: a new password, keeping app passwords and
/// API keys.
pub async fn set_password(context: &str, addr: &str, random: bool) -> Result<()> {
    let accounts = Accounts::load(context).await?;
    let account = accounts.find(addr)?;
    let email = accounts.email(account);
    let secret = password(random)?;
    let credential = json!({"@type": "Password", "secret": secret.as_str()});
    let others = items(&account["credentials"])
        .iter()
        .any(|c| c["@type"] != "Password");
    let key = account["credentials"]
        .as_object()
        .and_then(|c| c.iter().find(|(_, v)| v["@type"] == "Password"))
        .map(|(k, _)| k.clone());
    let patch = match key {
        // Only the password one changes, so the others keep their secrets.
        Some(k) if others => json!({format!("credentials/{k}"): credential}),
        _ if others => bail!(
            "{email} has app passwords or API keys but no password; set one at its account page"
        ),
        _ => json!({"credentials": list(vec![credential])}),
    };
    accounts
        .stalwart
        .update("x:Account", &id(account)?, patch)
        .await?;
    println!("changed the password of {email}");
    if random {
        println!("its password: {}", secret.as_str());
    }
    Ok(())
}

/// `uc mail account alias add` and `rm`: more addresses whose mail goes to
/// the account.
pub async fn change_aliases(
    context: &str,
    addr: &str,
    aliases: &[String],
    add: bool,
) -> Result<()> {
    let accounts = Accounts::load(context).await?;
    let account = accounts.find(addr)?;
    let email = accounts.email(account);
    let mut have = items(&account["aliases"]);
    let mut changed = Vec::new();
    for text in aliases {
        let (local, domain) = address(text)?;
        let alias = format!("{local}@{domain}");
        let domain_id = accounts.domains.id(&domain)?;
        let at = have
            .iter()
            .position(|x| x["name"] == local.as_str() && x["domainId"] == domain_id.as_str());
        match (add, at) {
            (true, Some(_)) => println!("{email} has {alias} already"),
            (true, None) => {
                if let Some(owner) = accounts.owner(&alias) {
                    bail!("{owner} has {alias} already");
                }
                have.push(json!({"name": local, "domainId": domain_id, "enabled": true}));
                changed.push(alias);
            }
            (false, Some(i)) => {
                have.remove(i);
                changed.push(alias);
            }
            (false, None) => println!("{email} has no alias {alias}"),
        }
    }
    if changed.is_empty() {
        return Ok(());
    }
    accounts
        .stalwart
        .update("x:Account", &id(account)?, json!({"aliases": list(have)}))
        .await?;
    let verb = if add {
        "now gets mail to"
    } else {
        "no longer gets mail to"
    };
    println!("{email} {verb} {}", changed.join(", "));
    Ok(())
}

/// `uc mail account quota`: how much mail the account may keep, or no
/// limit.
pub async fn set_quota(context: &str, addr: &str, quota: Option<u64>) -> Result<()> {
    let accounts = Accounts::load(context).await?;
    let account = accounts.find(addr)?;
    let email = accounts.email(account);
    accounts
        .stalwart
        .update(
            "x:Account",
            &id(account)?,
            json!({"quotas/maxDiskQuota": quota}),
        )
        .await?;
    match quota {
        Some(q) => println!("{email} may keep {} of mail", human(q)),
        None => println!("{email} may keep any amount of mail"),
    }
    Ok(())
}

/// A password: a random one, asked for twice on a terminal, or the first
/// line of stdin.
fn password(random: bool) -> Result<Zeroizing<String>> {
    if random {
        let charset = Charset {
            letters: true,
            digits: true,
            symbols: false,
        };
        return Ok(Zeroizing::new(password::generate(24, charset)?));
    }
    let secret = if std::io::stdin().is_terminal() {
        let first = crate::cli::prompt_password("Password: ")?;
        let again = crate::cli::prompt_password("Again: ")?;
        if *first != *again {
            bail!("the passwords differ");
        }
        first
    } else {
        let mut line = Zeroizing::new(String::new());
        std::io::stdin().lock().read_line(&mut line)?;
        Zeroizing::new(line.trim_end_matches(['\r', '\n']).to_string())
    };
    if secret.is_empty() {
        bail!("an empty password; type one, or pass --random");
    }
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_addresses() {
        assert_eq!(
            address(" Ada@Example.com ").unwrap(),
            ("ada".into(), "example.com".into())
        );
        assert!(address("ada").is_err());
        assert!(address("@example.com").is_err());
        assert!(address("ada@localhost").is_err());
    }

    #[test]
    fn reads_sizes() {
        assert_eq!(size("1024").unwrap(), 1024);
        assert_eq!(size("5G").unwrap(), 5 << 30);
        assert_eq!(size("500mb").unwrap(), 500 << 20);
        assert_eq!(size("2GiB").unwrap(), 2 << 30);
        assert!(size("5X").is_err());
        assert!(size("G").is_err());
    }

    #[test]
    fn lists_are_keyed_by_position() {
        let l = list(vec![json!("a"), json!("b")]);
        assert_eq!(l, json!({"0": "a", "1": "b"}));
        let shuffled = json!({"10": "k", "2": "c", "0": "a"});
        assert_eq!(items(&shuffled), vec![json!("a"), json!("c"), json!("k")]);
    }

    #[test]
    fn sizes_read_well() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KiB");
        assert_eq!(human(5 << 30), "5 GiB");
    }
}
