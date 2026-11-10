// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The console's account: sign up, log in and a forgotten password with an
//! email, a password and a mailed code; who's looking; and billing.
//!
//! The console's pages call these straight from the browser. Sessions are
//! cookies: `uc_otp` holds the code's token until it is typed in, on this
//! host only, and `uc_session` the session, an API key that runs out, set
//! for the console's domain so its own server can render pages for whoever
//! is logged in. Anything else (`uc`, scripts) sends `X-API-Key`.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{AppendHeaders, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::App;
use crate::accounts::{
    self, CODE_MINUTES, Checked, Created, Pending, Picked, Purpose, SESSION_DAYS, normalize_email,
    normalize_username, signup_problem, username_problem,
};
use crate::billing;
use crate::config::Settings;
use crate::error::{AppError, AppResult};
use crate::extract::{JsonBody, MaybeClient, SESSION_COOKIE, cookie};

/// The cookie holding the token of the code waiting to be typed in.
pub const OTP_COOKIE: &str = "uc_otp";
const VERIFY_PATH: &str = "/auth/email/verify";

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/account", get(account))
        .route("/account/username", post(pick_username))
        .route("/auth/email/signup", post(signup))
        .route("/auth/email/login", post(login))
        .route("/auth/email/reset", post(reset))
        .route(VERIFY_PATH, post(verify))
        .route("/billing/checkout", post(checkout))
        .route("/billing/portal", post(portal))
        .route("/stripe/webhook", post(webhook))
}

/// A `Set-Cookie` value. `shared` ones go to the console's domain too.
pub fn set_cookie(
    settings: &Settings,
    name: &str,
    value: &str,
    path: &str,
    max_age: i64,
    same_site: &str,
    shared: bool,
) -> HeaderValue {
    let mut cookie =
        format!("{name}={value}; Path={path}; Max-Age={max_age}; HttpOnly; SameSite={same_site}");
    if settings.secure_cookies() {
        cookie.push_str("; Secure");
    }
    if let (true, Some(domain)) = (shared, &settings.cookie_domain) {
        cookie.push_str(&format!("; Domain={domain}"));
    }
    HeaderValue::from_str(&cookie).expect("cookie values are tokens")
}

fn fail(status: StatusCode, message: &str) -> AppError {
    AppError::new(status, message)
}

const ON_OUR_SIDE: &str = "Something went wrong on our side. Please try again in a moment.";

/// Who's looking, and what billing they have: what the console's pages
/// need to show the right thing.
async fn account(
    State(app): State<Arc<App>>,
    MaybeClient(client): MaybeClient,
) -> AppResult<Response> {
    let billing = app.settings.stripe.is_some();
    let body = match client {
        None => {
            json!({"user": null, "billing": billing, "subscription": null, "subscribed": false})
        }
        Some(client) => {
            let sub = app.store.subscription(&client.user_id).await?;
            json!({
                "user": {
                    "email": client.email,
                    "phone": client.phone,
                    "username": client.username,
                },
                "billing": billing,
                "subscription": sub.as_ref().map(|s| json!({
                    "status": s.status,
                    "currentPeriodEnd": s.current_period_end.map(|t| t.to_rfc3339()),
                    "cancelAtPeriodEnd": s.cancel_at_period_end,
                })),
                "subscribed": billing::is_live(sub.as_ref()),
            })
        }
    };
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(body)).into_response())
}

#[derive(Deserialize, Default)]
struct Credentials {
    #[serde(default)]
    email: String,
    #[serde(default)]
    password: String,
    /// Only a sign up picks one.
    #[serde(default)]
    username: String,
}

impl Credentials {
    fn email(&self) -> String {
        normalize_email(&self.email)
    }

    fn username(&self) -> String {
        normalize_username(&self.username)
    }
}

const USERNAME_TAKEN: &str = "That username is taken. Try another.";

/// Mail a code for `pending` and keep its token in a cookie; the browser
/// then posts the code to the verify endpoint.
async fn challenge(app: &App, pending: Pending) -> AppResult<Response> {
    if app.limits.mailed.blocked(&pending.email) {
        return Err(fail(
            StatusCode::TOO_MANY_REQUESTS,
            "We've sent a few codes already. Check your inbox, or try again in a few minutes.",
        ));
    }
    let (token, code) = app.store.start_code(&pending).await?;
    let (subject, text) = accounts::message(pending.purpose, &code);
    // A reset for an address without an account: nothing to mail.
    let nobody = pending.purpose == Purpose::Reset && pending.user_id.is_none();
    if !nobody && let Err(err) = app.mailer.send(&pending.email, &subject, &text).await {
        tracing::error!("mailing a code to {}: {err:#}", pending.email);
        return Err(fail(
            StatusCode::SERVICE_UNAVAILABLE,
            "We couldn't send you the code just now. Please try again in a moment.",
        ));
    }
    app.limits.mailed.fail(&pending.email);
    let otp = set_cookie(
        &app.settings,
        OTP_COOKIE,
        &token,
        VERIFY_PATH,
        CODE_MINUTES * 60,
        "Strict",
        false,
    );
    Ok((
        StatusCode::ACCEPTED,
        [(header::SET_COOKIE, otp)],
        Json(json!({"code": true, "email": pending.email})),
    )
        .into_response())
}

/// Step one of a sign up: the email and password, then a code goes out.
async fn signup(
    State(app): State<Arc<App>>,
    JsonBody(body): JsonBody<Credentials>,
) -> AppResult<Response> {
    let email = body.email();
    let username = body.username();
    if let Some(problem) =
        username_problem(&username).or_else(|| signup_problem(&email, &body.password))
    {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, problem));
    }
    if app.store.find_email_user(&email).await?.is_some() {
        return Err(fail(
            StatusCode::CONFLICT,
            "That email already has an account. Try logging in.",
        ));
    }
    if app.store.find_username(&username).await?.is_some() {
        return Err(fail(StatusCode::CONFLICT, USERNAME_TAKEN));
    }
    let password_hash = accounts::hash_password_async(body.password).await?;
    challenge(
        &app,
        Pending {
            purpose: Purpose::Signup,
            email,
            user_id: None,
            password_hash: Some(password_hash),
            username: Some(username),
        },
    )
    .await
}

async fn login(
    State(app): State<Arc<App>>,
    JsonBody(body): JsonBody<Credentials>,
) -> AppResult<Response> {
    let email = body.email();
    if app.limits.login.blocked(&email) {
        return Err(fail(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many tries. Take a break and try again in a few minutes.",
        ));
    }
    let user = app.store.find_email_user(&email).await?;
    let stored = user.as_ref().and_then(|u| u.password_hash.clone());
    let right = accounts::verify_password_async(body.password, stored).await?;
    let Some(user) = user.filter(|_| right) else {
        app.limits.login.fail(&email);
        return Err(fail(
            StatusCode::UNAUTHORIZED,
            "That email and password don't match.",
        ));
    };
    challenge(
        &app,
        Pending {
            purpose: Purpose::Login,
            email: user.email,
            user_id: Some(user.id),
            password_hash: None,
            username: None,
        },
    )
    .await
}

/// A forgotten password: the address and a new password, then a code goes
/// out. An unknown address gets the same answer and no mail, so this
/// doesn't say who has an account.
async fn reset(
    State(app): State<Arc<App>>,
    JsonBody(body): JsonBody<Credentials>,
) -> AppResult<Response> {
    let email = body.email();
    if let Some(problem) = signup_problem(&email, &body.password) {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, problem));
    }
    let password_hash = accounts::hash_password_async(body.password).await?;
    let user = app.store.find_email_user(&email).await?;
    challenge(
        &app,
        Pending {
            purpose: Purpose::Reset,
            email,
            user_id: user.map(|u| u.id),
            password_hash: Some(password_hash),
            username: None,
        },
    )
    .await
}

#[derive(Deserialize, Default)]
struct CodeIn {
    #[serde(default)]
    code: String,
}

/// Step two: the code from the mail. A right one logs the browser in.
async fn verify(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    JsonBody(body): JsonBody<CodeIn>,
) -> AppResult<Response> {
    let settings = &app.settings;
    let token = cookie(&headers, OTP_COOKIE).unwrap_or_default();
    let clear_otp = set_cookie(settings, OTP_COOKIE, "", VERIFY_PATH, 0, "Strict", false);
    let pending = match app.store.check_code(&token, &body.code).await? {
        Checked::Wrong => {
            return Err(fail(
                StatusCode::UNAUTHORIZED,
                "That code isn't right. Check the mail and try again.",
            ));
        }
        Checked::Gone => {
            let gone = fail(
                StatusCode::GONE,
                "That code has run out. Go back and we'll send you a new one.",
            );
            return Ok(([(header::SET_COOKIE, clear_otp)], gone).into_response());
        }
        Checked::Ok(pending) => pending,
    };
    let user_id: Uuid = match pending.purpose {
        Purpose::Signup => {
            let hash = pending.password_hash.as_deref().unwrap_or_default();
            let username = pending.username.as_deref();
            match app
                .store
                .create_email_user(&pending.email, username, hash, Utc::now())
                .await?
            {
                Created::Made(id) => id,
                Created::EmailTaken => {
                    return Err(fail(
                        StatusCode::CONFLICT,
                        "That email already has an account. Try logging in.",
                    ));
                }
                Created::UsernameTaken => {
                    let taken = fail(
                        StatusCode::GONE,
                        "Someone just took that username. Go back and pick another.",
                    );
                    return Ok(([(header::SET_COOKIE, clear_otp)], taken).into_response());
                }
            }
        }
        Purpose::Login | Purpose::Reset => {
            let id = pending
                .user_id
                .ok_or_else(|| fail(StatusCode::GONE, ON_OUR_SIDE))?;
            if let (Purpose::Reset, Some(hash)) = (pending.purpose, &pending.password_hash) {
                app.store.set_password(&id, hash).await?;
            }
            app.limits.login.clear(&pending.email);
            id
        }
    };
    app.limits.mailed.clear(&pending.email);
    let expires = Utc::now() + chrono::Duration::days(SESSION_DAYS);
    let session = app.store.issue_session(&user_id, expires).await?;
    let session = set_cookie(
        settings,
        SESSION_COOKIE,
        &session,
        "/",
        SESSION_DAYS * 24 * 3600,
        "Lax",
        true,
    );
    let status = if pending.purpose == Purpose::Signup {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        AppendHeaders([
            (header::SET_COOKIE, clear_otp),
            (header::SET_COOKIE, session),
        ]),
        Json(json!({"email": pending.email})),
    )
        .into_response())
}

fn log_in_first() -> AppError {
    fail(StatusCode::UNAUTHORIZED, "Log in first.")
}

#[derive(Deserialize, Default)]
struct UsernameIn {
    #[serde(default)]
    username: String,
}

/// An account made before there were usernames picks one, once.
async fn pick_username(
    State(app): State<Arc<App>>,
    MaybeClient(client): MaybeClient,
    JsonBody(body): JsonBody<UsernameIn>,
) -> AppResult<Json<Value>> {
    let client = client.ok_or_else(log_in_first)?;
    let username = normalize_username(&body.username);
    if let Some(problem) = username_problem(&username) {
        return Err(fail(StatusCode::UNPROCESSABLE_ENTITY, problem));
    }
    match app.store.pick_username(&client.user_id, &username).await? {
        Picked::Set => Ok(Json(json!({"username": username}))),
        Picked::Taken => Err(fail(StatusCode::CONFLICT, USERNAME_TAKEN)),
        Picked::HasOne => Err(fail(
            StatusCode::CONFLICT,
            "Your account has a username already, and it stays as it is.",
        )),
    }
}

/// Where to send the browser to subscribe: Stripe's checkout, or the
/// account when they already have a live subscription.
async fn checkout(
    State(app): State<Arc<App>>,
    MaybeClient(client): MaybeClient,
) -> AppResult<Json<Value>> {
    let client = client.ok_or_else(log_in_first)?;
    let Some(stripe) = &app.settings.stripe else {
        return Err(fail(StatusCode::CONFLICT, "Subscriptions aren't open yet."));
    };
    let console = &app.settings.console_url;
    if billing::is_live(app.store.subscription(&client.user_id).await?.as_ref()) {
        return Ok(Json(json!({"url": format!("{console}/account")})));
    }
    let email = client.email.clone().unwrap_or_default();
    let url = billing::checkout_url(&app, stripe, &client.user_id, &email).await?;
    Ok(Json(json!({"url": url})))
}

/// Where the user manages their card and cancels.
async fn portal(
    State(app): State<Arc<App>>,
    MaybeClient(client): MaybeClient,
) -> AppResult<Json<Value>> {
    let client = client.ok_or_else(log_in_first)?;
    let back = format!("{}/account", app.settings.console_url);
    let url = match &app.settings.stripe {
        Some(stripe) => billing::portal_url(&app, stripe, &client.user_id).await?,
        None => None,
    };
    Ok(Json(json!({"url": url.unwrap_or(back)})))
}

/// Stripe signs the raw body; it comes from Stripe, not a browser.
async fn webhook(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    body: Bytes,
) -> AppResult<Json<Value>> {
    let Some(stripe) = &app.settings.stripe else {
        return Err(AppError::not_found("billing is off"));
    };
    let signature = headers
        .get("stripe-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !billing::verify_signature(
        &body,
        signature,
        &stripe.webhook_secret,
        Utc::now().timestamp(),
        300,
    ) {
        return Err(AppError::bad_request("bad signature"));
    }
    let event: billing::Event =
        serde_json::from_slice(&body).map_err(|_| AppError::bad_request("not an event"))?;
    billing::handle_event(&app, stripe, &event).await?;
    Ok(Json(json!({"received": true})))
}

#[cfg(test)]
mod tests {
    use axum::http::Method;
    use serde_json::json;

    use super::*;
    use crate::testing::{self, Browser};

    const ADA: &str = "ada@example.com";

    /// The code in the last mail to `to`.
    fn last_code(t: &testing::TestApp, to: &str) -> String {
        let mail = t.mail.lock().unwrap();
        let sent = mail.iter().rev().find(|m| m.to == to).expect("a mail");
        sent.subject[..6].to_string()
    }

    fn mails(t: &testing::TestApp) -> usize {
        t.mail.lock().unwrap().len()
    }

    async fn sign_up(t: &testing::TestApp, browser: &mut Browser, email: &str, password: &str) {
        let answer = browser
            .post(
                "/auth/email/signup",
                json!({"username": email.split('@').next(), "email": email, "password": password}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED, "{}", answer.body);
        let code = last_code(t, email);
        let answer = browser.post(VERIFY_PATH, json!({"code": code})).await;
        assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    }

    #[tokio::test]
    async fn sign_up_log_in_and_reset() {
        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let mut ada = Browser::new(&t.app);
        assert_eq!(ada.get("/account").await.body["user"], Value::Null);

        // A sign up: the details, then the code from the mail.
        let answer = ada
            .post(
                "/auth/email/signup",
                json!({"username": "ada", "email": " Ada@Example.com", "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED);
        assert_eq!(answer.body["email"], ADA);
        assert!(answer.cookie(OTP_COOKIE).is_some());
        let code = last_code(&t, ADA);
        let wrong = if code == "000000" { "111111" } else { "000000" };
        let answer = ada.post(VERIFY_PATH, json!({"code": wrong})).await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
        let answer = ada
            .post(
                VERIFY_PATH,
                json!({"code": format!("{}-{}", &code[..3], &code[3..])}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
        assert!(answer.cookie(SESSION_COOKIE).is_some());
        let me = ada.get("/account").await.body;
        assert_eq!(me["user"]["email"], ADA);
        assert_eq!(me["subscribed"], false);
        // The code is used up.
        assert_eq!(
            ada.post(VERIFY_PATH, json!({"code": code})).await.status,
            StatusCode::GONE
        );

        let mut other = Browser::new(&t.app);
        let answer = other
            .post(
                "/auth/email/signup",
                json!({"username": "ada", "email": ADA, "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::CONFLICT);
        let answer = other
            .post(
                "/auth/email/signup",
                json!({"username": "ada", "email": "nope", "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            answer.body["detail"],
            "That doesn't look like an email address."
        );

        // A log in somewhere else.
        let answer = other
            .post(
                "/auth/email/login",
                json!({"email": ADA, "password": "wrong password"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
        let answer = other
            .post(
                "/auth/email/login",
                json!({"email": ADA, "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED);
        let code = last_code(&t, ADA);
        let answer = other.post(VERIFY_PATH, json!({"code": code})).await;
        assert_eq!(answer.status, StatusCode::OK);
        assert_eq!(other.get("/account").await.body["user"]["email"], ADA);
        assert_eq!(other.get("/account").await.body["user"]["username"], "ada");

        // A reset logs out everywhere and takes the new password.
        let mut forgot = Browser::new(&t.app);
        let answer = forgot
            .post(
                "/auth/email/reset",
                json!({"email": ADA, "password": "a new password"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED);
        let code = last_code(&t, ADA);
        assert_eq!(
            forgot.post(VERIFY_PATH, json!({"code": code})).await.status,
            StatusCode::OK
        );
        assert_eq!(ada.get("/account").await.body["user"], Value::Null);
        assert_eq!(other.get("/account").await.body["user"], Value::Null);
        assert_eq!(forgot.get("/account").await.body["user"]["email"], ADA);
        let mut again = Browser::new(&t.app);
        let old = again
            .post(
                "/auth/email/login",
                json!({"email": ADA, "password": "0123456789"}),
            )
            .await;
        assert_eq!(old.status, StatusCode::UNAUTHORIZED);
        let new = again
            .post(
                "/auth/email/login",
                json!({"email": ADA, "password": "a new password"}),
            )
            .await;
        assert_eq!(new.status, StatusCode::ACCEPTED);

        // A reset for nobody looks the same and mails nothing.
        let before = mails(&t);
        let answer = Browser::new(&t.app)
            .post(
                "/auth/email/reset",
                json!({"email": "bob@example.com", "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED);
        assert_eq!(mails(&t), before);

        // Logging out ends the session.
        let answer = forgot.send(Method::POST, "/auth/logout", None).await;
        assert_eq!(answer.status, StatusCode::NO_CONTENT);
        assert_eq!(forgot.get("/account").await.body["user"], Value::Null);
    }

    #[tokio::test]
    async fn a_username_is_picked_once() {
        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let mut ada = Browser::new(&t.app);
        sign_up(&t, &mut ada, ADA, "0123456789").await;
        assert_eq!(ada.get("/account").await.body["user"]["username"], "ada");

        // Taken, or not a name: the sign up stops before any mail.
        let mut bob = Browser::new(&t.app);
        let before = mails(&t);
        let signup = |username: &str| json!({"username": username, "email": "bob@example.com", "password": "0123456789"});
        let answer = bob.post("/auth/email/signup", signup(" ADA ")).await;
        assert_eq!(answer.status, StatusCode::CONFLICT);
        assert_eq!(answer.body["detail"], USERNAME_TAKEN);
        for wrong in ["", "-bob", "bob--b", "b", "bob_b", "admin"] {
            let answer = bob.post("/auth/email/signup", signup(wrong)).await;
            assert_eq!(answer.status, StatusCode::UNPROCESSABLE_ENTITY, "{wrong:?}");
        }
        assert_eq!(mails(&t), before);

        // It never changes.
        let answer = ada
            .post("/account/username", json!({"username": "ada-2"}))
            .await;
        assert_eq!(answer.status, StatusCode::CONFLICT);
        let answer = Browser::new(&t.app)
            .post("/account/username", json!({"username": "ada-2"}))
            .await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);

        // An account from before usernames picks one on the account page.
        let hash = accounts::hash_password("0123456789").unwrap();
        let made = t
            .app
            .store
            .create_email_user("old@example.com", None, &hash, Utc::now())
            .await
            .unwrap();
        assert!(matches!(made, Created::Made(_)));
        let mut old = Browser::new(&t.app);
        let answer = old
            .post(
                "/auth/email/login",
                json!({"email": "old@example.com", "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED);
        let code = last_code(&t, "old@example.com");
        assert_eq!(
            old.post(VERIFY_PATH, json!({"code": code})).await.status,
            StatusCode::OK
        );
        assert_eq!(
            old.get("/account").await.body["user"]["username"],
            Value::Null
        );
        let answer = old
            .post("/account/username", json!({"username": "ada"}))
            .await;
        assert_eq!(answer.status, StatusCode::CONFLICT);

        // Bob's sign up waits for its code while the old account takes the
        // name: the code then says so, and no account is made.
        let answer = bob.post("/auth/email/signup", signup("old-timer")).await;
        assert_eq!(answer.status, StatusCode::ACCEPTED, "{}", answer.body);
        let answer = old
            .post("/account/username", json!({"username": " Old-Timer "}))
            .await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
        assert_eq!(answer.body["username"], "old-timer");
        assert_eq!(
            old.get("/account").await.body["user"]["username"],
            "old-timer"
        );
        let code = last_code(&t, "bob@example.com");
        let answer = bob.post(VERIFY_PATH, json!({"code": code})).await;
        assert_eq!(answer.status, StatusCode::GONE);
        assert!(
            t.app
                .store
                .find_email_user("bob@example.com")
                .await
                .unwrap()
                .is_none()
        );
        let answer = old
            .post("/account/username", json!({"username": "old-timer-2"}))
            .await;
        assert_eq!(answer.status, StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_code_takes_five_guesses() {
        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let mut browser = Browser::new(&t.app);
        browser
            .post(
                "/auth/email/signup",
                json!({"username": "ada", "email": ADA, "password": "0123456789"}),
            )
            .await;
        let code = last_code(&t, ADA);
        let wrong = if code == "000000" { "111111" } else { "000000" };
        for _ in 0..4 {
            let answer = browser.post(VERIFY_PATH, json!({"code": wrong})).await;
            assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
        }
        let answer = browser.post(VERIFY_PATH, json!({"code": wrong})).await;
        assert_eq!(answer.status, StatusCode::GONE);
        let answer = browser.post(VERIFY_PATH, json!({"code": code})).await;
        assert_eq!(answer.status, StatusCode::GONE);
    }

    #[tokio::test]
    async fn another_site_cant_post() {
        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let mut browser = Browser::new(&t.app);
        browser.origin = Some("https://evil.example".into());
        let answer = browser
            .post(
                "/auth/email/signup",
                json!({"username": "ada", "email": ADA, "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::FORBIDDEN);
        assert_eq!(mails(&t), 0);
        // Reading is fine; CORS keeps the answer from the page.
        assert_eq!(browser.get("/account").await.status, StatusCode::OK);
        // No browser, no Origin: uc and scripts.
        browser.origin = None;
        let answer = browser
            .post(
                "/auth/email/signup",
                json!({"username": "ada", "email": ADA, "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn billing_needs_a_login_and_stripe() {
        let Some(t) = testing::app(|_| {}).await else {
            return;
        };
        let mut browser = Browser::new(&t.app);
        let answer = browser.post("/billing/checkout", json!({})).await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
        sign_up(&t, &mut browser, ADA, "0123456789").await;
        let answer = browser.post("/billing/checkout", json!({})).await;
        assert_eq!(answer.status, StatusCode::CONFLICT);
        let answer = browser.post("/billing/portal", json!({})).await;
        assert_eq!(answer.body["url"], "http://localhost:8080/account");
        assert_eq!(browser.get("/account").await.body["billing"], false);
    }

    /// Stripe, as much of it as billing uses.
    async fn fake_stripe() -> String {
        use axum::extract::Path;
        let app = Router::new()
            .route(
                "/customers",
                post(|| async { Json(json!({"id": "cus_1"})) }),
            )
            .route(
                "/checkout/sessions",
                post(|| async { Json(json!({"url": "https://checkout.stripe.test/s"})) }),
            )
            .route(
                "/subscriptions/{id}",
                get(|Path(id): Path<String>| async move {
                    Json(json!({
                        "id": id, "customer": "cus_1", "status": "active",
                        "cancel_at_period_end": false,
                        "items": {"data": [{"current_period_end": 1_900_000_000}]},
                    }))
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });
        format!("http://{addr}")
    }

    fn signed(payload: &str, secret: &str) -> String {
        use hmac::{Hmac, Mac};
        let t = Utc::now().timestamp();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.{payload}").as_bytes());
        let sig: String = mac
            .finalize()
            .into_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("t={t},v1={sig}")
    }

    #[tokio::test]
    async fn subscribing_through_stripe() {
        let stripe = fake_stripe().await;
        let Some(t) = testing::app(|s| {
            s.stripe_api_base = stripe;
            s.stripe = Some(crate::config::Stripe {
                secret_key: "sk_test".into(),
                price_id: "price_1".into(),
                webhook_secret: "whsec_1".into(),
            });
        })
        .await
        else {
            return;
        };
        let mut browser = Browser::new(&t.app);
        sign_up(&t, &mut browser, ADA, "0123456789").await;
        let answer = browser.post("/billing/checkout", json!({})).await;
        assert_eq!(
            answer.body["url"], "https://checkout.stripe.test/s",
            "{}",
            answer.body
        );

        let event = json!({
            "id": "evt_1", "type": "customer.subscription.updated",
            "data": {"object": {"id": "sub_1"}},
        })
        .to_string();
        let hook = |signature: String| {
            axum::http::Request::post("/stripe/webhook")
                .header("stripe-signature", signature)
                .body(axum::body::Body::from(event.clone()))
                .unwrap()
        };
        let bad = testing::request_app(&t.app, hook("t=1,v1=00".into())).await;
        assert_eq!(bad.status, StatusCode::BAD_REQUEST);
        let ok = testing::request_app(&t.app, hook(signed(&event, "whsec_1"))).await;
        assert_eq!(ok.status, StatusCode::OK, "{}", ok.body);
        let me = browser.get("/account").await.body;
        assert_eq!(me["subscribed"], true);
        assert_eq!(me["subscription"]["status"], "active");
        // Sent again, it's the same event.
        let again = testing::request_app(&t.app, hook(signed(&event, "whsec_1"))).await;
        assert_eq!(again.status, StatusCode::OK);
        // Subscribed already: back to the account.
        let answer = browser.post("/billing/checkout", json!({})).await;
        assert_eq!(answer.body["url"], "http://localhost:8080/account");
    }
}
