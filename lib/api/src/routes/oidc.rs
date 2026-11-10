// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! OpenID Connect: other sites (Forgejo) log people in with usecode.dev.
//!
//! The authorization code flow, with PKCE when the site sends it:
//!
//! 1. The site sends the browser to `/oauth/authorize`. Someone logged in
//!    goes straight back with a code; anyone else logs in on the console
//!    first, and an account without a username picks one, then they're
//!    sent here again.
//! 2. The site's server swaps the code at `/oauth/token`, with its secret,
//!    for an ID token and an access token.
//! 3. It may read `/oauth/userinfo` with the access token.
//!
//! What the site gets is the account's id (`sub`), its email and its
//! username (see [`oidc::claims`]).

use std::sync::Arc;

use axum::extract::{Query, RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::{Form, Json, Router};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use url::Url;

use crate::App;
use crate::error::{AppError, AppResult};
use crate::extract::MaybeClient;
use crate::oidc::{self, Grant, Keys, TOKEN_SECONDS};

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/oauth/jwks", get(jwks))
        .route("/oauth/authorize", get(authorize))
        .route("/oauth/token", axum::routing::post(token))
        .route("/oauth/userinfo", get(userinfo).post(userinfo))
}

fn keys(app: &App) -> Keys {
    Keys::new(&app.settings.secret_key)
}

async fn discovery(State(app): State<Arc<App>>) -> Json<Value> {
    let issuer = &app.settings.public_url;
    Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/oauth/authorize"),
        "token_endpoint": format!("{issuer}/oauth/token"),
        "userinfo_endpoint": format!("{issuer}/oauth/userinfo"),
        "jwks_uri": format!("{issuer}/oauth/jwks"),
        "response_types_supported": ["code"],
        "grant_types_supported": ["authorization_code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["ES256"],
        "scopes_supported": ["openid", "email", "profile"],
        "claims_supported": [
            "sub", "iss", "aud", "exp", "iat", "nonce",
            "email", "email_verified", "preferred_username", "nickname", "name",
        ],
        "token_endpoint_auth_methods_supported": ["client_secret_basic", "client_secret_post"],
        "code_challenge_methods_supported": ["S256"],
        "authorization_response_iss_parameter_supported": true,
    }))
}

async fn jwks(State(app): State<Arc<App>>) -> Json<Value> {
    Json(json!({"keys": [keys(&app).jwk()]}))
}

#[derive(Deserialize, Default)]
struct AuthorizeIn {
    #[serde(default)]
    response_type: String,
    #[serde(default)]
    client_id: String,
    #[serde(default)]
    redirect_uri: String,
    #[serde(default)]
    scope: String,
    state: Option<String>,
    nonce: Option<String>,
    code_challenge: Option<String>,
    code_challenge_method: Option<String>,
    prompt: Option<String>,
}

/// Step one: back to the site with a code, once the browser is logged in
/// and the account has a username.
async fn authorize(
    State(app): State<Arc<App>>,
    MaybeClient(caller): MaybeClient,
    RawQuery(raw): RawQuery,
    Query(q): Query<AuthorizeIn>,
) -> AppResult<Response> {
    // Until the client and the address are known good, nothing goes back
    // to the address: it could be anyone's.
    let client = oidc::client(&app.settings, &q.client_id)?
        .ok_or_else(|| AppError::bad_request("That site doesn't log in with usecode.dev."))?;
    if !client.redirect_uris.contains(&q.redirect_uri) {
        return Err(AppError::bad_request(
            "That address isn't one this site may be sent back to.",
        ));
    }
    let back = |params: &[(&str, &str)]| -> AppResult<Response> {
        let mut url = Url::parse(&q.redirect_uri)
            .map_err(|_| AppError::bad_request("The address to go back to isn't a URL."))?;
        {
            let mut pairs = url.query_pairs_mut();
            for (k, v) in params {
                pairs.append_pair(k, v);
            }
            if let Some(state) = &q.state {
                pairs.append_pair("state", state);
            }
            pairs.append_pair("iss", &app.settings.public_url);
        }
        Ok(Redirect::to(url.as_str()).into_response())
    };
    let error = |code: &str, description: &str| {
        back(&[("error", code), ("error_description", description)])
    };

    if q.response_type != "code" {
        return error(
            "unsupported_response_type",
            "Only the code flow is supported.",
        );
    }
    if !q.scope.split_whitespace().any(|s| s == "openid") {
        return error("invalid_scope", "The openid scope is required.");
    }
    if q.code_challenge.is_some() && q.code_challenge_method.as_deref() != Some("S256") {
        return error(
            "invalid_request",
            "Only the S256 code challenge is supported.",
        );
    }

    // Here again once they've logged in, or picked a username.
    let again = format!(
        "{}/oauth/authorize?{}",
        app.settings.public_url,
        raw.unwrap_or_default()
    );
    let console = |page: &str| -> Response {
        let next: String = url::form_urlencoded::byte_serialize(again.as_bytes()).collect();
        Redirect::to(&format!("{}{page}?next={next}", app.settings.console_url)).into_response()
    };
    let Some(caller) = caller else {
        if q.prompt.as_deref() == Some("none") {
            return error("login_required", "Not logged in to usecode.dev.");
        }
        return Ok(console("/login"));
    };
    if caller.username.is_none() {
        if q.prompt.as_deref() == Some("none") {
            return error("interaction_required", "The account has no username yet.");
        }
        return Ok(console("/account"));
    }

    let code = app
        .store
        .start_oauth_code(&Grant {
            client_id: q.client_id.clone(),
            user_id: caller.user_id,
            redirect_uri: q.redirect_uri.clone(),
            scope: q.scope.clone(),
            nonce: q.nonce.clone(),
            code_challenge: q.code_challenge.clone(),
        })
        .await?;
    back(&[("code", &code)])
}

#[derive(Deserialize, Default)]
struct TokenIn {
    #[serde(default)]
    grant_type: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    redirect_uri: String,
    client_id: Option<String>,
    client_secret: Option<String>,
    code_verifier: Option<String>,
}

/// An OAuth error, as RFC 6749 has the token endpoint answer.
fn oauth_error(status: StatusCode, code: &str, description: &str) -> Response {
    let mut response = (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(json!({"error": code, "error_description": description})),
    )
        .into_response();
    if status == StatusCode::UNAUTHORIZED {
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            header::HeaderValue::from_static("Basic realm=\"usecode.dev\""),
        );
    }
    response
}

/// The client's id and secret, from HTTP Basic (each form-encoded, as RFC
/// 6749 has it) or the form.
fn client_credentials(headers: &HeaderMap, body: &TokenIn) -> Option<(String, String)> {
    let basic = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Basic "))
        .and_then(|v| STANDARD.decode(v.trim()).ok())
        .and_then(|v| String::from_utf8(v).ok());
    if let Some(basic) = basic {
        let (id, secret) = basic.split_once(':')?;
        let decode = |s: &str| -> String {
            url::form_urlencoded::parse(format!("v={s}").as_bytes())
                .next()
                .map(|(_, v)| v.into_owned())
                .unwrap_or_default()
        };
        return Some((decode(id), decode(secret)));
    }
    Some((body.client_id.clone()?, body.client_secret.clone()?))
}

/// Step two: the site's server swaps the code for the tokens.
async fn token(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    Form(body): Form<TokenIn>,
) -> AppResult<Response> {
    let bad_client = || {
        oauth_error(
            StatusCode::UNAUTHORIZED,
            "invalid_client",
            "Unknown client, or the wrong secret.",
        )
    };
    let Some((client_id, secret)) = client_credentials(&headers, &body) else {
        return Ok(bad_client());
    };
    match oidc::client(&app.settings, &client_id)? {
        Some(client) if client.is_secret(&secret) => {}
        _ => return Ok(bad_client()),
    }
    if body.grant_type != "authorization_code" {
        return Ok(oauth_error(
            StatusCode::BAD_REQUEST,
            "unsupported_grant_type",
            "Only authorization_code is supported.",
        ));
    }
    let bad_grant = || {
        oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "The code is wrong, used or run out.",
        )
    };
    // Taken even when the rest is wrong: a code is good for one try.
    let Some(grant) = app.store.take_oauth_code(&body.code).await? else {
        return Ok(bad_grant());
    };
    if grant.client_id != client_id || grant.redirect_uri != body.redirect_uri {
        return Ok(bad_grant());
    }
    if let Some(challenge) = &grant.code_challenge {
        let verifier = body.code_verifier.as_deref().unwrap_or_default();
        if oidc::s256(verifier) != *challenge {
            return Ok(bad_grant());
        }
    }
    let Some(who) = app.store.who(&grant.user_id).await? else {
        return Ok(bad_grant());
    };

    let keys = keys(&app);
    let issuer = &app.settings.public_url;
    let now = Utc::now().timestamp();
    let mut id = oidc::claims(&grant.user_id, &who);
    id.insert("iss".into(), json!(issuer));
    id.insert("aud".into(), json!(client_id));
    id.insert("azp".into(), json!(client_id));
    id.insert("iat".into(), json!(now));
    id.insert("exp".into(), json!(now + TOKEN_SECONDS));
    if let Some(nonce) = &grant.nonce {
        id.insert("nonce".into(), json!(nonce));
    }
    let access = json!({
        "iss": issuer,
        "sub": grant.user_id.to_string(),
        // For userinfo only: an ID token, made out to the client, isn't one.
        "aud": userinfo_audience(issuer),
        "client_id": client_id,
        "scope": grant.scope,
        "iat": now,
        "exp": now + TOKEN_SECONDS,
    });
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::PRAGMA, "no-cache"),
        ],
        Json(json!({
            "access_token": keys.sign(&access),
            "token_type": "Bearer",
            "expires_in": TOKEN_SECONDS,
            "id_token": keys.sign(&Value::Object(id)),
            "scope": grant.scope,
        })),
    )
        .into_response())
}

fn userinfo_audience(issuer: &str) -> String {
    format!("{issuer}/oauth/userinfo")
}

/// Step three, if the site wants it: the claims, for the access token.
async fn userinfo(State(app): State<Arc<App>>, headers: HeaderMap) -> AppResult<Response> {
    let unauthorized = || {
        (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer error=\"invalid_token\"")],
            Json(json!({"error": "invalid_token"})),
        )
            .into_response()
    };
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or_default();
    let Some(claims) = keys(&app).verify(token.trim()) else {
        return Ok(unauthorized());
    };
    let issuer = &app.settings.public_url;
    let live = claims.get("exp").and_then(Value::as_i64) > Some(Utc::now().timestamp());
    if !live
        || claims.get("iss").and_then(Value::as_str) != Some(issuer)
        || claims.get("aud").and_then(Value::as_str) != Some(&userinfo_audience(issuer))
    {
        return Ok(unauthorized());
    }
    let Some(user_id) = claims
        .get("sub")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
    else {
        return Ok(unauthorized());
    };
    let Some(who) = app.store.who(&user_id).await? else {
        return Ok(unauthorized());
    };
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        Json(Value::Object(oidc::claims(&user_id, &who))),
    )
        .into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;

    use super::*;
    use crate::testing::{self, Browser, TestApp};

    const CALLBACK: &str = "https://git.example.com/user/oauth2/usecode/callback";
    const SECRET: &str = "forgejo-secret";

    /// An app with Forgejo as a client.
    async fn app() -> Option<TestApp> {
        let dir = std::env::temp_dir().join(format!("oidc-clients-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("forgejo"),
            json!({"secret": SECRET, "redirect_uris": [CALLBACK]}).to_string(),
        )
        .unwrap();
        testing::app(|s| s.oidc_clients_dir = dir.display().to_string()).await
    }

    fn authorize_path(extra: &str) -> String {
        let callback: String = url::form_urlencoded::byte_serialize(CALLBACK.as_bytes()).collect();
        format!(
            "/oauth/authorize?response_type=code&client_id=forgejo&redirect_uri={callback}\
             &scope=openid%20email%20profile&state=xyz&nonce=n-1{extra}"
        )
    }

    fn location(answer: &testing::Answer) -> Url {
        let to = answer.headers[header::LOCATION].to_str().unwrap();
        Url::parse(to).unwrap()
    }

    fn param(url: &Url, name: &str) -> Option<String> {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    }

    async fn swap(t: &TestApp, form: &str, basic: Option<(&str, &str)>) -> testing::Answer {
        let mut request = Request::post("/oauth/token")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        if let Some((id, secret)) = basic {
            request = request.header(
                header::AUTHORIZATION,
                format!("Basic {}", STANDARD.encode(format!("{id}:{secret}"))),
            );
        }
        testing::request_app(&t.app, request.body(Body::from(form.to_string())).unwrap()).await
    }

    async fn sign_up(t: &TestApp, browser: &mut Browser) {
        let answer = browser
            .post(
                "/auth/email/signup",
                json!({"username": "ada", "email": "ada@example.com", "password": "0123456789"}),
            )
            .await;
        assert_eq!(answer.status, StatusCode::ACCEPTED, "{}", answer.body);
        let code = {
            let mail = t.mail.lock().unwrap();
            mail.last().unwrap().subject[..6].to_string()
        };
        let answer = browser
            .post("/auth/email/verify", json!({"code": code}))
            .await;
        assert_eq!(answer.status, StatusCode::CREATED, "{}", answer.body);
    }

    #[tokio::test]
    async fn forgejo_logs_ada_in() {
        let Some(t) = app().await else {
            return;
        };
        let discovery = Browser::new(&t.app)
            .get("/.well-known/openid-configuration")
            .await
            .body;
        assert_eq!(discovery["issuer"], "http://localhost:8000");
        assert_eq!(
            discovery["token_endpoint"],
            "http://localhost:8000/oauth/token"
        );

        // Not logged in: to the console's login, and back here after.
        let mut ada = Browser::new(&t.app);
        let verifier = "a-verifier-that-is-long-enough-for-pkce-0123456789";
        let pkce = format!(
            "&code_challenge={}&code_challenge_method=S256",
            oidc::s256(verifier)
        );
        let answer = ada.get(&authorize_path(&pkce)).await;
        assert_eq!(answer.status, StatusCode::SEE_OTHER);
        let login = location(&answer);
        assert_eq!(login.path(), "/login");
        let next = param(&login, "next").unwrap();
        assert!(
            next.starts_with("http://localhost:8000/oauth/authorize?"),
            "{next}"
        );

        sign_up(&t, &mut ada).await;
        let answer = ada
            .get(next.strip_prefix("http://localhost:8000").unwrap())
            .await;
        let back = location(&answer);
        assert_eq!(
            format!(
                "{}://{}{}",
                back.scheme(),
                back.host_str().unwrap(),
                back.path()
            ),
            CALLBACK
        );
        assert_eq!(param(&back, "state").as_deref(), Some("xyz"));
        assert_eq!(
            param(&back, "iss").as_deref(),
            Some("http://localhost:8000")
        );
        let code = param(&back, "code").unwrap();

        let callback: String = url::form_urlencoded::byte_serialize(CALLBACK.as_bytes()).collect();
        let form = |code: &str, verifier: &str| {
            format!(
                "grant_type=authorization_code&code={code}&redirect_uri={callback}\
                 &code_verifier={verifier}"
            )
        };
        // The wrong secret, then no PKCE verifier: the code is gone after a try.
        let answer = swap(&t, &form(&code, verifier), Some(("forgejo", "wrong"))).await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);
        assert_eq!(answer.body["error"], "invalid_client");
        let answer = swap(&t, &form(&code, verifier), Some(("forgejo", SECRET))).await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.body);
        let tokens = answer.body;
        let again = swap(&t, &form(&code, verifier), Some(("forgejo", SECRET))).await;
        assert_eq!(again.body["error"], "invalid_grant");

        let keys = Keys::new(&t.app.settings.secret_key);
        let id = keys.verify(tokens["id_token"].as_str().unwrap()).unwrap();
        assert_eq!(id["iss"], "http://localhost:8000");
        assert_eq!(id["aud"], "forgejo");
        assert_eq!(id["nonce"], "n-1");
        assert_eq!(id["email"], "ada@example.com");
        assert_eq!(id["email_verified"], true);
        assert_eq!(id["preferred_username"], "ada");
        let jwks = Browser::new(&t.app).get("/oauth/jwks").await.body;
        assert_eq!(jwks["keys"][0], keys.jwk());

        let userinfo = |token: String| {
            Request::get("/oauth/userinfo")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap()
        };
        let access = tokens["access_token"].as_str().unwrap().to_string();
        let me = testing::request_app(&t.app, userinfo(access)).await;
        assert_eq!(me.status, StatusCode::OK);
        assert_eq!(me.body["nickname"], "ada");
        assert_eq!(me.body["sub"], id["sub"]);
        // The ID token isn't an access token.
        let id_token = tokens["id_token"].as_str().unwrap().to_string();
        let answer = testing::request_app(&t.app, userinfo(id_token)).await;
        assert_eq!(answer.status, StatusCode::UNAUTHORIZED);

        // A second login, the secret in the form, with a wrong verifier.
        let answer = ada.get(&authorize_path(&pkce)).await;
        let code = param(&location(&answer), "code").unwrap();
        let mut wrong = form(&code, "not-the-verifier");
        wrong.push_str(&format!("&client_id=forgejo&client_secret={SECRET}"));
        assert_eq!(swap(&t, &wrong, None).await.body["error"], "invalid_grant");
    }

    #[tokio::test]
    async fn only_known_sites_and_their_addresses() {
        let Some(t) = app().await else {
            return;
        };
        let mut browser = Browser::new(&t.app);
        let answer = browser
            .get(&authorize_path("").replace("client_id=forgejo", "client_id=nobody"))
            .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        let answer = browser
            .get(&authorize_path("").replace("git.example.com", "evil.example"))
            .await;
        assert_eq!(answer.status, StatusCode::BAD_REQUEST);
        assert!(answer.headers.get(header::LOCATION).is_none());
        // A known site, asked wrong: the error goes back to it.
        let answer = browser
            .get(&authorize_path("").replace("scope=openid%20", "scope="))
            .await;
        assert_eq!(
            param(&location(&answer), "error").as_deref(),
            Some("invalid_scope")
        );
        let answer = browser.get(&authorize_path("&prompt=none")).await;
        assert_eq!(
            param(&location(&answer), "error").as_deref(),
            Some("login_required")
        );
        let answer = browser
            .get(&authorize_path(
                "&code_challenge=abc&code_challenge_method=plain",
            ))
            .await;
        assert_eq!(
            param(&location(&answer), "error").as_deref(),
            Some("invalid_request")
        );
    }

    #[tokio::test]
    async fn an_account_without_a_username_picks_one_first() {
        let Some(t) = app().await else {
            return;
        };
        let hash = crate::accounts::hash_password("0123456789").unwrap();
        t.app
            .store
            .create_email_user("old@example.com", None, &hash, Utc::now())
            .await
            .unwrap();
        let mut old = Browser::new(&t.app);
        old.post(
            "/auth/email/login",
            json!({"email": "old@example.com", "password": "0123456789"}),
        )
        .await;
        let code = {
            let mail = t.mail.lock().unwrap();
            mail.last().unwrap().subject[..6].to_string()
        };
        old.post("/auth/email/verify", json!({"code": code})).await;
        let answer = old.get(&authorize_path("")).await;
        let to = location(&answer);
        assert_eq!(to.path(), "/account");
        assert!(param(&to, "next").unwrap().contains("/oauth/authorize?"));
        old.post("/account/username", json!({"username": "old"}))
            .await;
        let answer = old.get(&authorize_path("")).await;
        assert!(param(&location(&answer), "code").is_some());
    }
}
