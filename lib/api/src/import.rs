// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc-agent-api import-console URL`: copy what the console kept in its own
//! database, before it used this API, into this one.
//!
//! Accounts keep their password hashes, live sessions keep working (a
//! session's token hash is an API key's hash here), and the Stripe
//! customers, subscriptions and handled events come along so billing
//! carries on. Mailed codes are short-lived and stay behind. Running it
//! again skips what is already here, so it can be run once before the
//! console moves over and once after, to catch what happened in between.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::Connection;
use sqlx::postgres::PgConnection;
use uuid::Uuid;

use crate::accounts::Created;
use crate::config::Settings;
use crate::db::Db;
use crate::migrate;
use crate::store::Store;

pub async fn run(settings: Settings, source: &str) -> Result<()> {
    migrate::run(&settings).await?;
    let db = Arc::new(Db::connect(&settings).await?);
    db.seed_shard_ranges().await?;
    let store = Store::new(db.clone());
    let mut src = PgConnection::connect(source)
        .await
        .context("connecting to the console's database")?;
    let counts = copy(&store, &mut src).await?;
    println!(
        "imported {} new account(s) of {}, {} session(s), {} subscription(s), {} Stripe event(s)",
        counts.new_users, counts.users, counts.sessions, counts.subscriptions, counts.events
    );
    db.close().await;
    Ok(())
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub users: usize,
    pub new_users: usize,
    pub sessions: usize,
    pub subscriptions: usize,
    pub events: usize,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn copy(store: &Store, src: &mut PgConnection) -> Result<Counts> {
    let mut counts = Counts::default();

    let users: Vec<(i64, String, String, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, email, password_hash, created_at FROM users ORDER BY id")
            .fetch_all(&mut *src)
            .await
            .context("reading the console's users")?;
    let mut ids: HashMap<i64, Uuid> = HashMap::new();
    for (old, email, password_hash, created_at) in users {
        counts.users += 1;
        let id = match store.find_email_user(&email).await? {
            Some(user) => user.id,
            None => match store
                .create_email_user(&email, None, &password_hash, created_at)
                .await?
            {
                Created::Made(id) => {
                    counts.new_users += 1;
                    id
                }
                // Without a username to clash, only the email can.
                Created::EmailTaken | Created::UsernameTaken => {
                    store
                        .find_email_user(&email)
                        .await?
                        .context("an account that is there and isn't")?
                        .id
                }
            },
        };
        ids.insert(old, id);
    }

    let sessions: Vec<(Vec<u8>, i64, DateTime<Utc>)> = sqlx::query_as(
        "SELECT token_hash, user_id, expires_at FROM sessions WHERE expires_at > now()",
    )
    .fetch_all(&mut *src)
    .await
    .context("reading the console's sessions")?;
    for (token_hash, old, expires_at) in sessions {
        let Some(id) = ids.get(&old) else { continue };
        store
            .insert_api_key(id, &hex(&token_hash), "session", Some(expires_at))
            .await?;
        counts.sessions += 1;
    }

    type Sub = (
        i64,
        String,
        Option<String>,
        String,
        Option<DateTime<Utc>>,
        bool,
    );
    let subs: Vec<Sub> = sqlx::query_as(
        "SELECT user_id, stripe_customer_id, stripe_subscription_id, status,
                current_period_end, cancel_at_period_end
         FROM subscriptions",
    )
    .fetch_all(&mut *src)
    .await
    .context("reading the console's subscriptions")?;
    for (old, customer, subscription_id, status, current_period_end, cancel_at_period_end) in subs {
        let Some(id) = ids.get(&old) else { continue };
        let fresh = store.stripe_customer(id).await?.is_none();
        store.save_stripe_customer(id, &customer).await?;
        if let (true, Some(subscription_id)) = (fresh, subscription_id) {
            let state = crate::billing::Subscription {
                status,
                current_period_end,
                cancel_at_period_end,
            };
            store
                .update_subscription(id, &customer, &subscription_id, &state)
                .await?;
        }
        counts.subscriptions += 1;
    }

    let events: Vec<String> = sqlx::query_scalar("SELECT id FROM stripe_events")
        .fetch_all(&mut *src)
        .await
        .context("reading the console's Stripe events")?;
    for event in events {
        store.first_stripe_event(&event).await?;
        counts.events += 1;
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use axum::http::{Request, StatusCode};
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::testing;

    /// The console's own schema, as its migrations left it.
    const CONSOLE: &str = include_str!("../testdata/console_schema.sql");

    #[tokio::test]
    async fn the_console_moves_over() {
        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let Some((_, console)) = testing::fresh_database().await else {
            return;
        };
        let mut src = PgConnection::connect_with(&console).await.unwrap();
        sqlx::raw_sql(CONSOLE).execute(&mut src).await.unwrap();
        let token = "a-console-session-token";
        let token_hash = Sha256::digest(token.as_bytes()).to_vec();
        sqlx::raw_sql(
            "INSERT INTO users (email, password_hash) VALUES
                 ('ada@example.com', 'scrypt$32768$8$1$zxREC_OdG3ab9b9wy1aDoQ$iofY9IrjxUq4zXlK4EbMZHsR__LMqySBLVsyfxx6gpU'),
                 ('bob@example.com', 'scrypt$32768$8$1$x$y');
             INSERT INTO subscriptions (user_id, stripe_customer_id, stripe_subscription_id, status)
                 SELECT id, 'cus_ada', 'sub_ada', 'active' FROM users WHERE email = 'ada@example.com';
             INSERT INTO stripe_events (id) VALUES ('evt_old');",
        )
        .execute(&mut src)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO sessions (token_hash, user_id, expires_at)
             SELECT $1, id, now() + interval '1 day' FROM users WHERE email = 'ada@example.com'",
        )
        .bind(&token_hash)
        .execute(&mut src)
        .await
        .unwrap();

        let counts = copy(&t.app.store, &mut src).await.unwrap();
        assert_eq!(
            counts,
            Counts {
                users: 2,
                new_users: 2,
                sessions: 1,
                subscriptions: 1,
                events: 1
            }
        );

        // Ada is still logged in, subscribed, and her password works.
        let me = testing::request_app(
            &t.app,
            Request::get("/account")
                .header("cookie", format!("uc_session={token}"))
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await;
        assert_eq!(me.body["user"]["email"], "ada@example.com");
        assert_eq!(me.body["subscription"]["status"], "active");
        let ada = t
            .app
            .store
            .find_email_user("ada@example.com")
            .await
            .unwrap()
            .unwrap();
        assert!(crate::accounts::verify_password(
            "correct horse battery",
            ada.password_hash.as_deref().unwrap()
        ));
        assert!(!t.app.store.first_stripe_event("evt_old").await.unwrap());

        // Again: nothing new.
        let counts = copy(&t.app.store, &mut src).await.unwrap();
        assert_eq!(counts.new_users, 0);
        assert_eq!(me.status, StatusCode::OK);
    }
}
