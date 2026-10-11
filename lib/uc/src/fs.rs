// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc fs`: buckets and objects on the S3 object storage the `rustfs` role
//! of `uc kube configure` runs, or any S3 endpoint.
//!
//! Requests are signed here (AWS Signature Version 4, path-style), so no
//! `aws`, `mc` or `rclone` is needed. The endpoint and the keys come from
//! `UC_FS_ENDPOINT`, `UC_FS_ACCESS_KEY` and `UC_FS_SECRET_KEY` when they are
//! set, else from the cluster: the rustfs Ingress's name and the
//! rustfs/rustfs-root secret.

use crate::kube::remote::kubectl_json;
use anyhow::{Context, Result, anyhow, bail};
use base64::Engine;
use hmac::{Hmac, KeyInit, Mac};
use reqwest::{Method, StatusCode};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

const NAMESPACE: &str = "rustfs";
const INGRESS: &str = "rustfs";
const ROOT_SECRET: &str = "rustfs-root";
/// RustFS, like MinIO, takes any region; this is the one they report.
const REGION: &str = "us-east-1";
/// Where RustFS's admin API is.
const ADMIN: &str = "/rustfs/admin/v3";

/// The HTTP client: no overall timeout, since an object can be big.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // graviola is pure Rust, see `kube::cloudflare::client`.
        let _ = rustls_graviola::default_provider().install_default();
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(120))
            .build()
            .expect("HTTP client")
    })
}

/// An S3 endpoint and the keys for it.
pub struct Store {
    /// `https://s3.example.com`, without a trailing slash.
    endpoint: String,
    access_key: String,
    secret_key: String,
}

/// An object's key and size, from a listing.
pub struct Object {
    pub key: String,
    pub size: u64,
    pub modified: String,
}

/// What a listing found under a prefix.
#[derive(Default)]
pub struct Listing {
    /// "Directories": the common prefixes, ending with the delimiter.
    pub prefixes: Vec<String>,
    pub objects: Vec<Object>,
}

impl Store {
    /// From the environment, else from the cluster behind `context`.
    pub async fn connect(context: impl FnOnce() -> Result<String>) -> Result<Store> {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let (endpoint, access, secret) = (
            env("UC_FS_ENDPOINT"),
            env("UC_FS_ACCESS_KEY"),
            env("UC_FS_SECRET_KEY"),
        );
        if let (Some(endpoint), Some(access_key), Some(secret_key)) =
            (endpoint.clone(), access.clone(), secret.clone())
        {
            return Ok(Store::new(&endpoint, &access_key, &secret_key));
        }
        let context = context()?;
        let endpoint = match endpoint {
            Some(e) => e,
            None => {
                let ingress =
                    kubectl_json(&context, &["-n", NAMESPACE, "get", "ingress", INGRESS]).await?;
                let host = ingress["spec"]["rules"][0]["host"]
                    .as_str()
                    .ok_or_else(|| anyhow!("{NAMESPACE}/{INGRESS} has no host"))?;
                format!("https://{host}")
            }
        };
        let (access_key, secret_key) = match (access, secret) {
            (Some(a), Some(s)) => (a, s),
            _ => {
                let s = kubectl_json(&context, &["-n", NAMESPACE, "get", "secret", ROOT_SECRET])
                    .await
                    .context("reading the RustFS keys; is `uc kube configure -t rustfs` done?")?;
                (
                    secret_field(&s, "access-key")?,
                    secret_field(&s, "secret-key")?,
                )
            }
        };
        Ok(Store::new(&endpoint, &access_key, &secret_key))
    }

    pub fn new(endpoint: &str, access_key: &str, secret_key: &str) -> Store {
        let endpoint = endpoint.trim().trim_end_matches('/');
        let endpoint = if endpoint.contains("://") {
            endpoint.to_string()
        } else {
            format!("https://{endpoint}")
        };
        Store {
            endpoint,
            access_key: access_key.trim().to_string(),
            secret_key: secret_key.trim().to_string(),
        }
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Sends a signed request for `path` (`/bucket/key`, unencoded) and
    /// fails on anything but a success.
    async fn call(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, &str)],
        body: Vec<u8>,
    ) -> Result<reqwest::Response> {
        let what = format!("{method} {path}");
        let uri = encode_path(path);
        let query = canonical_query(query);
        let host = self
            .endpoint
            .split_once("://")
            .map_or(self.endpoint.as_str(), |(_, h)| h);
        let payload = hex(&Sha256::digest(&body));
        let (amz_date, signature_headers) =
            self.sign(&method, host, &uri, &query, &payload, SystemTime::now());
        let mut url = format!("{}{uri}", self.endpoint);
        if !query.is_empty() {
            url = format!("{url}?{query}");
        }
        let reply = client()
            .request(method, &url)
            .header("x-amz-date", amz_date)
            .header("x-amz-content-sha256", payload)
            .header("authorization", signature_headers)
            .body(body)
            .send()
            .await
            .with_context(|| format!("{what} on {}", self.endpoint))?;
        if reply.status().is_success() {
            return Ok(reply);
        }
        let status = reply.status();
        let text = reply.text().await.unwrap_or_default();
        let why = tag(&text, "Message")
            .or_else(|| tag(&text, "Code"))
            .unwrap_or_else(|| text.trim().to_string());
        if status == StatusCode::NOT_FOUND && why.is_empty() {
            bail!("{what}: not found");
        }
        bail!("{what}: HTTP {status}: {why}")
    }

    /// The `x-amz-date` and `authorization` headers for a request.
    fn sign(
        &self,
        method: &Method,
        host: &str,
        uri: &str,
        query: &str,
        payload: &str,
        now: SystemTime,
    ) -> (String, String) {
        let amz_date = amz_date(now);
        let day = &amz_date[..8];
        let signed = "host;x-amz-content-sha256;x-amz-date";
        let canonical = format!(
            "{method}\n{uri}\n{query}\nhost:{host}\nx-amz-content-sha256:{payload}\n\
             x-amz-date:{amz_date}\n\n{signed}\n{payload}"
        );
        let scope = format!("{day}/{REGION}/s3/aws4_request");
        let to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
            hex(&Sha256::digest(canonical.as_bytes()))
        );
        let key = [day, REGION, "s3", "aws4_request"].iter().fold(
            format!("AWS4{}", self.secret_key).into_bytes(),
            |k, part| hmac(&k, part.as_bytes()),
        );
        let signature = hex(&hmac(&key, to_sign.as_bytes()));
        let auth = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed}, Signature={signature}",
            self.access_key
        );
        (amz_date, auth)
    }

    pub async fn buckets(&self) -> Result<Vec<(String, String)>> {
        let text = self
            .call(Method::GET, "/", &[], Vec::new())
            .await?
            .text()
            .await?;
        let doc = roxmltree::Document::parse(&text).context("reading the bucket list")?;
        Ok(doc
            .descendants()
            .filter(|n| n.has_tag_name("Bucket"))
            .map(|b| (child(b, "Name"), child(b, "CreationDate")))
            .collect())
    }

    pub async fn make_bucket(&self, bucket: &str) -> Result<()> {
        self.call(Method::PUT, &format!("/{bucket}"), &[], Vec::new())
            .await?;
        Ok(())
    }

    pub async fn remove_bucket(&self, bucket: &str) -> Result<()> {
        self.call(Method::DELETE, &format!("/{bucket}"), &[], Vec::new())
            .await?;
        Ok(())
    }

    /// Everything under `prefix`, all of it with `recursive`, else one
    /// level, as a directory listing would.
    pub async fn list(&self, bucket: &str, prefix: &str, recursive: bool) -> Result<Listing> {
        let mut listing = Listing::default();
        let mut token = String::new();
        loop {
            let mut query = vec![("list-type", "2"), ("prefix", prefix)];
            if !recursive {
                query.push(("delimiter", "/"));
            }
            if !token.is_empty() {
                query.push(("continuation-token", &token));
            }
            let path = format!("/{bucket}");
            let text = self
                .call(Method::GET, &path, &query, Vec::new())
                .await?
                .text()
                .await?;
            let doc = roxmltree::Document::parse(&text).context("reading the object list")?;
            for n in doc.root_element().children() {
                if n.has_tag_name("Contents") {
                    listing.objects.push(Object {
                        key: child(n, "Key"),
                        size: child(n, "Size").parse().unwrap_or(0),
                        modified: child(n, "LastModified"),
                    });
                } else if n.has_tag_name("CommonPrefixes") {
                    listing.prefixes.push(child(n, "Prefix"));
                }
            }
            let truncated = child(doc.root_element(), "IsTruncated") == "true";
            token = child(doc.root_element(), "NextContinuationToken");
            if !truncated || token.is_empty() {
                return Ok(listing);
            }
        }
    }

    pub async fn put(&self, bucket: &str, key: &str, body: Vec<u8>) -> Result<()> {
        self.call(Method::PUT, &format!("/{bucket}/{key}"), &[], body)
            .await?;
        Ok(())
    }

    /// Writes the object to `out` as it arrives, and returns its size.
    pub async fn get(
        &self,
        bucket: &str,
        key: &str,
        out: &mut (impl AsyncWriteExt + Unpin),
    ) -> Result<u64> {
        let mut reply = self
            .call(Method::GET, &format!("/{bucket}/{key}"), &[], Vec::new())
            .await?;
        let mut size = 0;
        while let Some(chunk) = reply.chunk().await? {
            out.write_all(&chunk).await?;
            size += chunk.len() as u64;
        }
        out.flush().await?;
        Ok(size)
    }

    pub async fn remove(&self, bucket: &str, key: &str) -> Result<()> {
        self.call(Method::DELETE, &format!("/{bucket}/{key}"), &[], Vec::new())
            .await?;
        Ok(())
    }

    /// A call to RustFS's admin API, signed with root keys. Under its own
    /// prefix it takes plain JSON; MinIO's `/minio/admin` wants it encrypted.
    async fn admin(
        &self,
        method: Method,
        op: &str,
        query: &[(&str, &str)],
        body: Vec<u8>,
    ) -> Result<String> {
        let path = format!("{ADMIN}/{op}");
        Ok(self.call(method, &path, query, body).await?.text().await?)
    }

    /// The users, by access key, each with its `policyName` and `status`.
    pub async fn users(&self) -> Result<Value> {
        let text = self
            .admin(Method::GET, "list-users", &[], Vec::new())
            .await?;
        serde_json::from_str(&text).context("reading the user list")
    }

    /// The policies, by name.
    pub async fn policies(&self) -> Result<Value> {
        let text = self
            .admin(Method::GET, "list-canned-policies", &[], Vec::new())
            .await?;
        serde_json::from_str(&text).context("reading the policy list")
    }

    /// Adds the policy `name`, or replaces it.
    pub async fn add_policy(&self, name: &str, policy: &Value) -> Result<()> {
        let body = serde_json::to_vec(policy)?;
        self.admin(Method::PUT, "add-canned-policy", &[("name", name)], body)
            .await?;
        Ok(())
    }

    /// Adds the user `access_key`, or sets its secret key, enabled.
    pub async fn add_user(&self, access_key: &str, secret_key: &str) -> Result<()> {
        let body = serde_json::to_vec(&json!({"secretKey": secret_key, "status": "enabled"}))?;
        self.admin(Method::PUT, "add-user", &[("accessKey", access_key)], body)
            .await?;
        Ok(())
    }

    /// Gives `user` the policy `policy`, in place of the one it had.
    pub async fn set_user_policy(&self, user: &str, policy: &str) -> Result<()> {
        let query = [
            ("policyName", policy),
            ("userOrGroup", user),
            ("isGroup", "false"),
        ];
        self.admin(Method::PUT, "set-user-or-group-policy", &query, Vec::new())
            .await?;
        Ok(())
    }
}

/// The bucket and the key of `<bucket>:<key>` (or `s3://<bucket>/<key>`),
/// the key maybe empty; `None` for a local path. As with scp, a bucket name
/// has no `/` in it, so `./a:b` is a local file.
pub fn remote(path: &str) -> Option<(String, String)> {
    let (bucket, key) = match path.strip_prefix("s3://") {
        Some(rest) => rest.split_once('/').unwrap_or((rest, "")),
        None => path.split_once(':')?,
    };
    if bucket.is_empty() || bucket.contains('/') {
        return None;
    }
    Some((bucket.to_string(), key.trim_start_matches('/').to_string()))
}

/// Splits `bucket:key`, `bucket/key` or `s3://bucket/key` into the bucket
/// and the key, which may be empty.
pub fn split(path: &str) -> Result<(String, String)> {
    if let Some(parts) = remote(path) {
        return Ok(parts);
    }
    let path = path.trim_start_matches('/');
    let (bucket, key) = path.split_once('/').unwrap_or((path, ""));
    if bucket.is_empty() {
        bail!("no bucket in '{path}'; use <bucket>:<key>");
    }
    Ok((bucket.to_string(), key.to_string()))
}

/// `field` of a Secret read as JSON, decoded.
pub fn secret_field(secret: &Value, field: &str) -> Result<String> {
    let raw = secret["data"][field].as_str().ok_or_else(|| {
        let meta = &secret["metadata"];
        anyhow!(
            "{}/{} has no {field}",
            meta["namespace"].as_str().unwrap_or(NAMESPACE),
            meta["name"].as_str().unwrap_or(ROOT_SECRET)
        )
    })?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(raw)?;
    Ok(String::from_utf8(bytes)?.trim().to_string())
}

fn child(node: roxmltree::Node, name: &str) -> String {
    node.children()
        .find(|c| c.has_tag_name(name))
        .and_then(|c| c.text())
        .unwrap_or_default()
        .to_string()
}

/// The text of the first `<name>` in an XML error body.
fn tag(xml: &str, name: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let text = doc.descendants().find(|n| n.has_tag_name(name))?.text()?;
    Some(text.trim().to_string()).filter(|t| !t.is_empty())
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Percent-encodes everything but S3's unreserved characters (and `/` when
/// `slash` is set).
fn encode(text: &str, slash: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) || (slash && b == b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn encode_path(path: &str) -> String {
    encode(path, true)
}

/// The query, encoded and sorted, as SigV4 signs it and as it is sent.
fn canonical_query(query: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(String, String)> = query
        .iter()
        .map(|(k, v)| (encode(k, false), encode(v, false)))
        .collect();
    pairs.sort();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// `YYYYMMDDTHHMMSSZ` in UTC.
fn amz_date(now: SystemTime) -> String {
    let secs = now.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rem) = (secs / 86_400, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// A byte count the way people read it: 512 B, 1.5 KiB, 3.2 MiB.
pub fn human(bytes: u64) -> String {
    let units = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < units.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", units[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_utc() {
        let t = UNIX_EPOCH + Duration::from_secs(1_369_353_600);
        assert_eq!(amz_date(t), "20130524T000000Z");
        let t = UNIX_EPOCH + Duration::from_secs(951_782_400 + 3_661);
        assert_eq!(amz_date(t), "20000229T010101Z");
    }

    #[test]
    fn signs_like_aws() {
        // The ListObjects example of the AWS SigV4 documentation, which
        // signs the same three headers.
        let store = Store::new(
            "https://examplebucket.s3.amazonaws.com",
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        let empty = hex(&Sha256::digest(b""));
        let (_, auth) = store.sign(
            &Method::GET,
            "examplebucket.s3.amazonaws.com",
            "/",
            &canonical_query(&[("max-keys", "2"), ("prefix", "J")]),
            &empty,
            UNIX_EPOCH + Duration::from_secs(1_369_353_600),
        );
        assert!(auth.ends_with(
            "Signature=34b48302e7b5fa45bde8084f4b7868a86f0a534bc59db6670ed5711ef69dc6f7"
        ));
    }

    #[test]
    fn keys_are_encoded_but_keep_their_slashes() {
        assert_eq!(encode_path("/b/a dir/x+y.txt"), "/b/a%20dir/x%2By.txt");
        assert_eq!(
            canonical_query(&[("prefix", "a/b"), ("list-type", "2")]),
            "list-type=2&prefix=a%2Fb"
        );
    }

    #[test]
    fn splits_bucket_and_key() {
        assert_eq!(split("b/k/x").unwrap(), ("b".into(), "k/x".into()));
        assert_eq!(split("s3://b").unwrap(), ("b".into(), String::new()));
        assert_eq!(split("b:k/x").unwrap(), ("b".into(), "k/x".into()));
        assert_eq!(split("b:").unwrap(), ("b".into(), String::new()));
        assert!(split("/").is_err());
    }

    #[test]
    fn local_paths_are_not_remote() {
        assert_eq!(remote("tehran:"), Some(("tehran".into(), String::new())));
        assert_eq!(remote("tehran:a/b"), Some(("tehran".into(), "a/b".into())));
        assert_eq!(remote("scripts/"), None);
        assert_eq!(remote("./a:b"), None);
        assert_eq!(remote(":x"), None);
    }
}
