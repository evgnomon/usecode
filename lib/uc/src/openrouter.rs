// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc agent usage`: the OpenRouter usage of the key `uc` itself chats with.
//!
//! OpenRouter knows two accounts: the user's, the credits purchased and spent
//! across all their keys (`GET /api/v1/credits`, a management key), and the
//! one key doing the talking (`GET /api/v1/key`). Here the key is the one
//! `openrouter` provider in `agent.yaml` or `--key` gives, the same one the coding agent uses.
//!
//! The key's own view is shown first: the credits it spent today, this week,
//! this month and all time, its BYOK usage, its credit limit and remaining,
//! and the budgets that can stop it before the account balance does; then the
//! account's balance, if the key can read it, and the free-model requests
//! left for the day.

use anyhow::{Context, Result};
use serde_json::Value;
use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::Duration;

/// The OpenRouter key the usage is read for: the `openrouter` provider's in `agent.yaml`, else the
/// one `--key` gives. The coding agent itself takes the same key, so this
/// shows what `uc` is spending.
pub struct OpenRouter {
    key: String,
}

/// The usage of one OpenRouter key: all the fields `uc agent usage` shows,
/// already lifted out of the responses.
#[derive(Debug, Default)]
pub struct Usage {
    label: String,
    usage: f64,
    usage_daily: f64,
    usage_weekly: f64,
    usage_monthly: f64,
    byok_usage: f64,
    limit: Option<f64>,
    /// The type of window the key's credit limit resets in.
    limit_reset: Option<String>,
    limit_remaining: Option<f64>,
    budgets: Vec<Budget>,
    free_tier: Option<bool>,
    /// The free-model requests of the current UTC day.
    free_model: Option<FreeModel>,
    credits: Option<Credits>,
}

/// One budget that applies to requests made with the key: a spend cap with
/// its own reset window, narrower than the account balance.
#[derive(Debug, Default)]
pub struct Budget {
    scope: String,
    guardrail_id: Option<String>,
    limit_usd: f64,
    spend_usd: f64,
    remaining_usd: f64,
    reset_interval: Option<String>,
    resets_at: Option<String>,
}

/// The free-model requests of the current UTC day: used of the day's limit,
/// and what is left of it.
#[derive(Debug, Default)]
pub struct FreeModel {
    used: u64,
    limit: u64,
    remaining: u64,
}

/// The account's credits: everything purchased, and everything spent across
/// all the user's keys. What is left of it is the account balance.
#[derive(Debug, Default)]
pub struct Credits {
    total_credits: f64,
    total_usage: f64,
}

impl OpenRouter {
    pub fn new(key: String) -> Self {
        Self { key }
    }

    /// The usage of the key, from both of OpenRouter's endpoints. The key's
    /// own usage is what this is for, so a key that cannot read the
    /// account's credits (OpenRouter asks for a management key there) still
    /// shows its own; a wrong key fails both and the error names it.
    pub async fn usage(&self) -> Result<Usage> {
        let mut usage = Usage::default();
        match self.get("credits").await {
            Ok(reply) => {
                usage.credits = Some(Credits {
                    total_credits: reply["data"]["total_credits"].as_f64().unwrap_or_default(),
                    total_usage: reply["data"]["total_usage"].as_f64().unwrap_or_default(),
                });
            }
            // OpenRouter refuses the account's credits to a key that is not
            // a management key; the key's own usage is still shown.
            Err(err) if refused(&err) => (),
            Err(err) => return Err(err),
        }
        usage.key_data(self.get("key").await.context("reading the key's usage")?["data"].clone());
        Ok(usage)
    }

    /// One OpenRouter endpoint, `GET /api/v1/<what>`, as JSON.
    async fn get(&self, what: &'static str) -> Result<Value> {
        let reply = client()
            .get(format!("https://openrouter.ai/api/v1/{what}"))
            .bearer_auth(&self.key)
            .send()
            .await
            .with_context(|| format!("calling OpenRouter's {what}"))?;
        let status = reply.status();
        let value: Value = reply
            .json()
            .await
            .with_context(|| format!("reading OpenRouter's {what}"))?;
        if !status.is_success() {
            anyhow::bail!(
                "HTTP {status}: {}",
                value["error"]["message"].as_str().unwrap_or("no message")
            );
        }
        Ok(value)
    }
}

impl Usage {
    /// Fills in the key's own usage from `GET /api/v1/key`'s `data`.
    fn key_data(&mut self, data: Value) {
        self.label = data["label"].as_str().unwrap_or_default().to_string();
        self.usage = data["usage"].as_f64().unwrap_or_default();
        self.usage_daily = data["usage_daily"].as_f64().unwrap_or_default();
        self.usage_weekly = data["usage_weekly"].as_f64().unwrap_or_default();
        self.usage_monthly = data["usage_monthly"].as_f64().unwrap_or_default();
        self.byok_usage = data["byok_usage"].as_f64().unwrap_or_default();
        self.limit = data["limit"].as_f64();
        self.limit_reset = data["limit_reset"].as_str().map(str::to_string);
        self.limit_remaining = data["limit_remaining"].as_f64();
        self.free_tier = data["is_free_tier"].as_bool();
        if let Some(free) = data["free_model_daily_requests"].as_object() {
            self.free_model = Some(FreeModel {
                used: free["used"].as_u64().unwrap_or_default(),
                limit: free["limit"].as_u64().unwrap_or_default(),
                remaining: free["remaining"].as_u64().unwrap_or_default(),
            });
        }
        self.budgets = data["budgets"]
            .as_array()
            .map(|budgets| {
                budgets
                    .iter()
                    .map(|b| Budget {
                        scope: b["scope"].as_str().unwrap_or_default().to_string(),
                        guardrail_id: b["guardrail_id"].as_str().map(str::to_string),
                        limit_usd: b["limit_usd"].as_f64().unwrap_or_default(),
                        spend_usd: b["spend_usd"].as_f64().unwrap_or_default(),
                        remaining_usd: b["remaining_usd"].as_f64().unwrap_or_default(),
                        reset_interval: b["reset_interval"].as_str().map(str::to_string),
                        resets_at: b["resets_at"].as_str().map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    /// The plain text `uc agent usage` prints: the key's spend and limits,
    /// the account's balance, and the free-model requests left today.
    pub fn report(&self) -> String {
        let mut out = String::new();
        let key = if self.label.is_empty() {
            "API key"
        } else {
            &self.label
        };
        let _ = writeln!(out, "{key}");
        let _ = writeln!(
            out,
            "  spent:        {} today, {} this week, {} this month, {} all time",
            usd(self.usage_daily),
            usd(self.usage_weekly),
            usd(self.usage_monthly),
            usd(self.usage)
        );
        if self.byok_usage > 0.0 {
            let _ = writeln!(out, "  byok usage:   {} all time", usd(self.byok_usage));
        }
        if self.limit.is_some() || self.limit_remaining.is_some() {
            let limit = self.limit.map_or("none".to_string(), usd);
            let reset = self
                .limit_reset
                .clone()
                .unwrap_or_else(|| "never".to_string());
            let remaining = self.limit_remaining.map_or("no limit".to_string(), usd);
            let _ = writeln!(out, "  key limit:    {limit} ({reset}), {remaining} left");
        }
        for budget in &self.budgets {
            let mut scope = budget.scope.replace('_', " ");
            if let Some(id) = &budget.guardrail_id {
                scope = format!("{scope} {id}");
            }
            let reset = budget.reset_interval.as_deref().unwrap_or("never");
            let _ = writeln!(
                out,
                "  budget:       {scope} ({reset}): {} of {}, {} left, resets {}",
                usd(budget.spend_usd),
                usd(budget.limit_usd),
                usd(budget.remaining_usd),
                budget.resets_at.as_deref().unwrap_or("never")
            );
        }
        if let Some(credits) = &self.credits {
            let balance = credits.total_credits - credits.total_usage;
            let _ = writeln!(
                out,
                "  account:      {} left of {} purchased, {} spent",
                usd(balance),
                usd(credits.total_credits),
                usd(credits.total_usage)
            );
        }
        if let Some(free) = &self.free_model {
            let _ = writeln!(
                out,
                "  free models:  {} of {} requests today, {} left",
                free.used, free.limit, free.remaining
            );
        }
        if self.free_tier == Some(true) {
            let _ = writeln!(out, "  tier:         free tier");
        }
        out
    }
}

/// Whether OpenRouter refused the endpoint for the key's permissions, rather
/// than answering or the network failing: the account's credits need a
/// management key.
fn refused(err: &anyhow::Error) -> bool {
    let text = format!("{err:#}");
    text.contains("403") || text.contains("401") || text.contains("Only management keys")
}

/// Dollars, as OpenRouter reports them.
fn usd(v: f64) -> String {
    format!("${v:.2}")
}

/// The HTTP client: graviola is pure Rust, see `chat::client`.
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let _ = rustls_graviola::default_provider().install_default();
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(120))
            .build()
            .expect("HTTP client")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = r#"{
        "data": {
            "label": "My Production Key",
            "usage": 25.5,
            "usage_daily": 1.25,
            "usage_weekly": 10.0,
            "usage_monthly": 20.0,
            "byok_usage": 17.38,
            "limit": 50,
            "limit_remaining": 48.75,
            "limit_reset": "daily",
            "is_free_tier": true,
            "free_model_daily_requests": {"used": 12, "limit": 50, "remaining": 38},
            "budgets": [
                {"scope": "workspace_default_member", "guardrail_id": null,
                 "limit_usd": 100, "spend_usd": 40, "remaining_usd": 60,
                 "reset_interval": "weekly", "resets_at": "2026-10-12T00:00:00.000Z"}
            ]
        }
    }"#;

    fn key_usage() -> Usage {
        let mut usage = Usage::default();
        usage.key_data(serde_json::from_str::<Value>(KEY).unwrap()["data"].clone());
        usage
    }

    #[test]
    fn reads_the_key() {
        let usage = key_usage();
        assert_eq!(usage.label, "My Production Key");
        assert_eq!(usage.usage, 25.5);
        assert_eq!(usage.usage_daily, 1.25);
        assert_eq!(usage.usage_weekly, 10.0);
        assert_eq!(usage.usage_monthly, 20.0);
        assert_eq!(usage.byok_usage, 17.38);
        assert_eq!(usage.limit, Some(50.0));
        assert_eq!(usage.limit_reset.as_deref(), Some("daily"));
        assert_eq!(usage.limit_remaining, Some(48.75));
        assert_eq!(usage.free_tier, Some(true));
        assert_eq!(usage.free_model.as_ref().map(|f| f.remaining), Some(38));
        assert_eq!(usage.budgets.len(), 1);
        assert_eq!(usage.budgets[0].scope, "workspace_default_member");
        assert_eq!(usage.budgets[0].remaining_usd, 60.0);
    }

    #[test]
    fn renders_the_report() {
        let mut usage = key_usage();
        usage.credits = Some(Credits {
            total_credits: 100.5,
            total_usage: 25.75,
        });
        let report = usage.report();
        assert!(report.contains("My Production Key"));
        assert!(report.contains("spent:        $1.25 today, $10.00 this week"));
        assert!(report.contains("$20.00 this month, $25.50 all time"));
        assert!(report.contains("byok usage:   $17.38 all time"));
        assert!(report.contains("key limit:    $50.00 (daily), $48.75 left"));
        assert!(report.contains("budget:       workspace default member (weekly)"));
        assert!(report.contains("account:      $74.75 left of $100.50 purchased"));
        assert!(report.contains("free models:  12 of 50 requests today, 38 left"));
        assert!(report.contains("tier:         free tier"));
    }

    #[test]
    fn empty_usage_renders_its_zeroes() {
        let report = Usage::default().report();
        assert!(report.contains("API key"));
        assert!(report.contains("spent:        $0.00 today"));
        assert!(!report.contains("account:"));
        assert!(!report.contains("budget:"));
    }

    #[test]
    fn missing_fields_are_defaults() {
        let mut usage = Usage::default();
        usage.key_data(serde_json::json!({"label": "thin"}));
        assert_eq!(usage.label, "thin");
        assert_eq!(usage.usage, 0.0);
        assert_eq!(usage.limit, None);
        assert!(usage.budgets.is_empty());
        assert!(usage.free_model.is_none());
        let report = usage.report();
        assert!(!report.contains("key limit"));
        assert!(!report.contains("budget:"));
        assert!(!report.contains("free models"));
    }
}
