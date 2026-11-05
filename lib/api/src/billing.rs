// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The Hosted plan: $10 a month per user, any number of nodes. Individual
//! accounts are free and never come here.
//!
//! Stripe holds the truth. Checkout takes the payment, the customer portal
//! handles card changes and cancelling, and the webhook copies the
//! subscription's state here. The subscription is read back from Stripe
//! rather than taken from the event, so events arriving late or out of
//! order still leave its latest state.

use anyhow::{Context, Result, bail};
use chrono::{DateTime, TimeZone, Utc};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::App;
use crate::config::Stripe;
use crate::schema;
use crate::store::Store;

/// The events the webhook endpoint in Stripe has to send.
#[allow(dead_code)]
pub const EVENTS: [&str; 4] = [
    "checkout.session.completed",
    "customer.subscription.created",
    "customer.subscription.updated",
    "customer.subscription.deleted",
];

/// A subscription in one of these keeps the user's nodes supervised.
const LIVE: [&str; 3] = ["active", "trialing", "past_due"];

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Subscription {
    pub status: String,
    pub current_period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
}

pub fn is_live(sub: Option<&Subscription>) -> bool {
    sub.is_some_and(|s| LIVE.contains(&s.status.as_str()))
}

async fn call(
    app: &App,
    stripe: &Stripe,
    method: reqwest::Method,
    path: &str,
    params: &[(&str, String)],
) -> Result<Value> {
    let url = format!("{}{path}", app.settings.stripe_api_base);
    let request = app
        .http
        .request(method.clone(), &url)
        .bearer_auth(&stripe.secret_key)
        .timeout(std::time::Duration::from_secs(15));
    let request = if method == reqwest::Method::GET {
        request.query(params)
    } else {
        request.form(params)
    };
    let response = request
        .send()
        .await
        .with_context(|| format!("Stripe {method} {path}"))?;
    let status = response.status();
    let json: Value = response.json().await.unwrap_or(Value::Null);
    if !status.is_success() {
        let why = json["error"]["message"].as_str().unwrap_or_default();
        bail!("Stripe {method} {path}: {status} {why}");
    }
    Ok(json)
}

impl Store {
    /// The user's subscription, once they have one.
    pub async fn subscription(&self, user_id: &Uuid) -> Result<Option<Subscription>> {
        let pool = self.pool_for_user(user_id).await?;
        Ok(sqlx::query_as(
            "SELECT status, current_period_end, cancel_at_period_end FROM subscriptions
             WHERE user_id = $1 AND stripe_subscription_id IS NOT NULL",
        )
        .bind(user_id)
        .fetch_optional(&pool)
        .await?)
    }

    pub async fn stripe_customer(&self, user_id: &Uuid) -> Result<Option<String>> {
        let pool = self.pool_for_user(user_id).await?;
        Ok(
            sqlx::query_scalar("SELECT stripe_customer_id FROM subscriptions WHERE user_id = $1")
                .bind(user_id)
                .fetch_optional(&pool)
                .await?,
        )
    }

    /// Note that `customer` is `user_id`'s; the first one saved for a user
    /// wins. The directory goes first, so a webhook can always find the
    /// user of a customer the user has.
    pub async fn save_stripe_customer(&self, user_id: &Uuid, customer: &str) -> Result<()> {
        let directory = self
            .pool_for_key(schema::STRIPE_CUSTOMERS, customer)
            .await?;
        sqlx::query(
            "INSERT INTO stripe_customers (stripe_customer_id, user_id, created_at)
             VALUES ($1, $2, $3) ON CONFLICT DO NOTHING",
        )
        .bind(customer)
        .bind(user_id)
        .bind(Utc::now())
        .execute(&directory)
        .await?;
        let pool = self.pool_for_user(user_id).await?;
        sqlx::query(
            "INSERT INTO subscriptions (user_id, stripe_customer_id, updated_at)
             VALUES ($1, $2, $3) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(customer)
        .bind(Utc::now())
        .execute(&pool)
        .await?;
        Ok(())
    }

    pub async fn stripe_customer_user(&self, customer: &str) -> Result<Option<Uuid>> {
        let directory = self
            .pool_for_key(schema::STRIPE_CUSTOMERS, customer)
            .await?;
        Ok(
            sqlx::query_scalar(
                "SELECT user_id FROM stripe_customers WHERE stripe_customer_id = $1",
            )
            .bind(customer)
            .fetch_optional(&directory)
            .await?,
        )
    }

    /// Whether the event is new; it is marked seen either way.
    pub async fn first_stripe_event(&self, id: &str) -> Result<bool> {
        let pool = self.pool_for_key(schema::STRIPE_EVENTS, id).await?;
        let fresh = sqlx::query(
            "INSERT INTO stripe_events (id, received_at) VALUES ($1, $2) ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(Utc::now())
        .execute(&pool)
        .await?;
        Ok(fresh.rows_affected() > 0)
    }

    pub async fn update_subscription(
        &self,
        user_id: &Uuid,
        customer: &str,
        subscription_id: &str,
        sub: &Subscription,
    ) -> Result<()> {
        let pool = self.pool_for_user(user_id).await?;
        sqlx::query(
            "UPDATE subscriptions SET stripe_subscription_id = $3, status = $4,
                 current_period_end = $5, cancel_at_period_end = $6, updated_at = $7
             WHERE user_id = $1 AND stripe_customer_id = $2",
        )
        .bind(user_id)
        .bind(customer)
        .bind(subscription_id)
        .bind(&sub.status)
        .bind(sub.current_period_end)
        .bind(sub.cancel_at_period_end)
        .bind(Utc::now())
        .execute(&pool)
        .await?;
        Ok(())
    }
}

/// The user's Stripe customer, made the first time they subscribe.
async fn customer(app: &App, stripe: &Stripe, user_id: &Uuid, email: &str) -> Result<String> {
    if let Some(known) = app.store.stripe_customer(user_id).await? {
        return Ok(known);
    }
    let made = call(
        app,
        stripe,
        reqwest::Method::POST,
        "/customers",
        &[
            ("email", email.to_string()),
            ("metadata[user_id]", user_id.to_string()),
        ],
    )
    .await?;
    let id = made["id"]
        .as_str()
        .context("Stripe made a customer without an id")?;
    app.store.save_stripe_customer(user_id, id).await?;
    // Two clicks at once: the first customer saved wins.
    Ok(app
        .store
        .stripe_customer(user_id)
        .await?
        .unwrap_or_else(|| id.to_string()))
}

/// Where to send the user to pay for the subscription.
pub async fn checkout_url(
    app: &App,
    stripe: &Stripe,
    user_id: &Uuid,
    email: &str,
) -> Result<String> {
    let console = &app.settings.console_url;
    let session = call(
        app,
        stripe,
        reqwest::Method::POST,
        "/checkout/sessions",
        &[
            ("mode", "subscription".into()),
            ("customer", customer(app, stripe, user_id, email).await?),
            ("client_reference_id", user_id.to_string()),
            ("line_items[0][price]", stripe.price_id.clone()),
            ("line_items[0][quantity]", "1".into()),
            ("allow_promotion_codes", "true".into()),
            ("success_url", format!("{console}/account?subscribed=1")),
            ("cancel_url", format!("{console}/pricing")),
        ],
    )
    .await?;
    Ok(session["url"].as_str().unwrap_or_default().to_string())
}

/// Where the user manages their card and cancels, or `None` before they
/// have subscribed.
pub async fn portal_url(app: &App, stripe: &Stripe, user_id: &Uuid) -> Result<Option<String>> {
    let Some(customer) = app.store.stripe_customer(user_id).await? else {
        return Ok(None);
    };
    let session = call(
        app,
        stripe,
        reqwest::Method::POST,
        "/billing_portal/sessions",
        &[
            ("customer", customer),
            (
                "return_url",
                format!("{}/account", app.settings.console_url),
            ),
        ],
    )
    .await?;
    Ok(session["url"].as_str().map(str::to_string))
}

/// Whether `header` (Stripe-Signature) signs `payload` with `secret`,
/// within `tolerance` seconds of `now`.
pub fn verify_signature(
    payload: &[u8],
    header: &str,
    secret: &str,
    now: i64,
    tolerance: i64,
) -> bool {
    let parts: Vec<(&str, &str)> = header
        .split(',')
        .filter_map(|p| p.trim().split_once('='))
        .collect();
    let Some(t) = parts
        .iter()
        .find(|(k, _)| *k == "t")
        .and_then(|(_, v)| v.parse::<i64>().ok())
    else {
        return false;
    };
    if (now - t).abs() > tolerance {
        return false;
    }
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(format!("{t}.").as_bytes());
    mac.update(payload);
    let want = mac.finalize().into_bytes();
    parts
        .iter()
        .filter(|(k, _)| *k == "v1")
        .filter_map(|(_, v)| hex(v))
        .any(|got| bool::from(got.ct_eq(want.as_slice())))
}

fn hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[derive(Deserialize)]
pub struct Event {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub data: EventData,
}

#[derive(Deserialize)]
pub struct EventData {
    pub object: Value,
}

/// The subscription an event is about, if any.
fn subscription_id(event: &Event) -> Option<String> {
    let object = &event.data.object;
    let id = if event.kind == "checkout.session.completed" {
        object["subscription"].as_str()
    } else if event.kind.starts_with("customer.subscription.") {
        object["id"].as_str()
    } else {
        None
    };
    id.map(str::to_string)
}

/// Copy what a webhook event changed. Each event is handled once.
pub async fn handle_event(app: &App, stripe: &Stripe, event: &Event) -> Result<()> {
    let Some(id) = subscription_id(event) else {
        app.store.first_stripe_event(&event.id).await?;
        return Ok(());
    };
    let sub = call(
        app,
        stripe,
        reqwest::Method::GET,
        &format!("/subscriptions/{id}"),
        &[],
    )
    .await?;
    // Seen only once the subscription is read: should Stripe not answer,
    // the event fails and Stripe sends it again.
    if !app.store.first_stripe_event(&event.id).await? {
        return Ok(());
    }
    let customer = sub["customer"].as_str().unwrap_or_default();
    let Some(user_id) = app.store.stripe_customer_user(customer).await? else {
        tracing::warn!(
            "Stripe event {} is about customer {customer:?}, who isn't ours",
            event.id
        );
        return Ok(());
    };
    let end = sub["items"]["data"][0]["current_period_end"]
        .as_i64()
        .or_else(|| sub["current_period_end"].as_i64());
    let state = Subscription {
        status: sub["status"].as_str().unwrap_or("incomplete").to_string(),
        current_period_end: end.and_then(|s| Utc.timestamp_opt(s, 0).single()),
        cancel_at_period_end: sub["cancel_at_period_end"].as_bool().unwrap_or(false),
    };
    app.store
        .update_subscription(
            &user_id,
            customer,
            sub["id"].as_str().unwrap_or(&id),
            &state,
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(payload: &[u8], secret: &str, t: i64) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.").as_bytes());
        mac.update(payload);
        let sig: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("t={t},v1={sig}")
    }

    #[test]
    fn a_signature_is_checked() {
        let payload = br#"{"id":"evt_1"}"#;
        let header = sign(payload, "whsec_x", 1000);
        assert!(verify_signature(payload, &header, "whsec_x", 1100, 300));
        assert!(!verify_signature(payload, &header, "whsec_y", 1100, 300));
        assert!(!verify_signature(b"{}", &header, "whsec_x", 1100, 300));
        assert!(!verify_signature(payload, &header, "whsec_x", 2000, 300));
        assert!(!verify_signature(payload, "v1=00", "whsec_x", 1000, 300));
    }

    #[test]
    fn live_statuses() {
        let sub = |status: &str| Subscription {
            status: status.into(),
            current_period_end: None,
            cancel_at_period_end: false,
        };
        assert!(is_live(Some(&sub("active"))));
        assert!(is_live(Some(&sub("past_due"))));
        assert!(!is_live(Some(&sub("canceled"))));
        assert!(!is_live(None));
    }
}
