// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! For the tests that need Postgres: a database of their own on the server
//! at `USECODE_AGENT_TEST_DATABASE_URL` (`make test-db`), and the whole app
//! on one. Without that server they skip themselves.

use std::env;
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use serde_json::Value;
use sqlx::Connection;
use sqlx::postgres::{PgConnectOptions, PgConnection};
use tower::ServiceExt;

use crate::config::Settings;
use crate::db::Db;
use crate::mail::{Mailer, Sent};
use crate::store::Store;
use crate::{App, accounts, chat_store, crypto, migrate, routes, web};

/// A new, empty database: its URL and its options.
pub async fn fresh_database() -> Option<(String, PgConnectOptions)> {
    let url = env::var("USECODE_AGENT_TEST_DATABASE_URL").ok()?;
    let options: PgConnectOptions = url.parse().unwrap();
    let mut admin = match PgConnection::connect_with(&options).await {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("skipped, no test database: {err}");
            return None;
        }
    };
    let name = format!("test_{}", uuid::Uuid::new_v4().simple());
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&mut admin)
        .await
        .unwrap();
    let (base, _) = url.rsplit_once('/').unwrap();
    Some((format!("{base}/{name}"), options.database(&name)))
}

/// The app on a fresh database, and what it mailed.
pub struct TestApp {
    pub app: Arc<App>,
    pub mail: Arc<Mutex<Vec<Sent>>>,
}

pub async fn app(change: impl FnOnce(&mut Settings)) -> Option<TestApp> {
    let (url, _) = fresh_database().await?;
    let mut settings = Settings::from_env().unwrap();
    settings.node_name = "test".into();
    settings.database_url = url;
    settings.shards.clear();
    settings.console_url = "http://localhost:8080".into();
    settings.cookie_domain = None;
    settings.smtp = None;
    settings.stripe = None;
    change(&mut settings);
    migrate::run(&settings).await.unwrap();
    let db = Arc::new(Db::connect(&settings).await.unwrap());
    db.seed_shard_ranges().await.unwrap();
    let mail = Arc::new(Mutex::new(Vec::new()));
    let app = Arc::new(App {
        cipher: crypto::Cipher::new(&settings.secret_key),
        store: Store::new(db),
        chat: chat_store::ChatStore::default(),
        templates: web::Templates::new().unwrap(),
        mailer: Mailer::Memory(mail.clone()),
        limits: accounts::Limits::default(),
        http: reqwest::Client::new(),
        settings,
    });
    Some(TestApp { app, mail })
}

/// What came back from a request.
pub struct Answer {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Value,
}

impl Answer {
    /// The `Set-Cookie` values, as `(name, value, max_age)`.
    pub fn cookies(&self) -> Vec<(String, String, Option<i64>)> {
        self.headers
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .filter_map(|v| {
                let mut parts = v.split(';').map(str::trim);
                let (name, value) = parts.next()?.split_once('=')?;
                let max_age = parts
                    .find_map(|p| p.strip_prefix("Max-Age="))
                    .and_then(|n| n.parse().ok());
                Some((name.to_string(), value.to_string(), max_age))
            })
            .collect()
    }

    pub fn cookie(&self, name: &str) -> Option<String> {
        self.cookies()
            .into_iter()
            .find(|(n, v, age)| n == name && !v.is_empty() && *age != Some(0))
            .map(|(_, v, _)| v)
    }
}

/// A browser of sorts: it keeps cookies and sends them back.
pub struct Browser {
    pub app: Arc<App>,
    pub cookies: Vec<(String, String)>,
    pub origin: Option<String>,
}

impl Browser {
    pub fn new(app: &Arc<App>) -> Self {
        Self {
            app: app.clone(),
            cookies: Vec::new(),
            origin: Some("http://localhost:8080".into()),
        }
    }

    pub async fn send(&mut self, method: Method, path: &str, body: Option<Value>) -> Answer {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(origin) = &self.origin {
            request = request.header(header::ORIGIN, origin);
        }
        if !self.cookies.is_empty() {
            let jar: Vec<String> = self
                .cookies
                .iter()
                .map(|(n, v)| format!("{n}={v}"))
                .collect();
            request = request.header(header::COOKIE, jar.join("; "));
        }
        let body = match body {
            Some(json) => {
                request = request.header(header::CONTENT_TYPE, "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let answer = request_app(&self.app, request.body(body).unwrap()).await;
        for (name, value, max_age) in answer.cookies() {
            self.cookies.retain(|(n, _)| *n != name);
            if !value.is_empty() && max_age != Some(0) {
                self.cookies.push((name, value));
            }
        }
        answer
    }

    pub async fn get(&mut self, path: &str) -> Answer {
        self.send(Method::GET, path, None).await
    }

    pub async fn post(&mut self, path: &str, body: Value) -> Answer {
        self.send(Method::POST, path, Some(body)).await
    }
}

pub async fn request_app(app: &Arc<App>, request: Request<Body>) -> Answer {
    let response = routes::router(app.clone()).oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Answer {
        status,
        headers,
        body,
    }
}
