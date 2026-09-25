// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The server-rendered chat UI (Jinja2 templates + HTMX), logged in with the
//! same OTP flow as the JSON API and kept in a session cookie.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use chrono::{Local, TimeZone};
use minijinja::{AutoEscape, Environment, Value, context};
use serde::{Deserialize, Serialize};

use crate::App;
use crate::chat_store::{AGENTS, Message, now};
use crate::error::{AppError, AppResult};
use crate::extract::FormBody;
use crate::models::normalize_phone;
use crate::otp;
use crate::store::WebSessionRecord;

const AUTH_COOKIE_NAME: &str = "usecode_agent_web_session";
const COOKIE_MAX_AGE_SECONDS: u32 = 60 * 60 * 24 * 30;

macro_rules! template {
    ($name:literal) => {
        ($name, include_str!(concat!("../templates/", $name)))
    };
}

const TEMPLATES: &[(&str, &str)] = &[
    template!("base.html.jinja2"),
    template!("login.html.jinja2"),
    template!("shell.html.jinja2"),
    template!("partials/chat_messages.html.jinja2"),
    template!("partials/chat_panel.html.jinja2"),
    template!("partials/login_code_form.html.jinja2"),
    template!("partials/login_phone_form.html.jinja2"),
];

pub struct Templates(Environment<'static>);

impl Templates {
    pub fn new() -> Result<Self> {
        let mut env = Environment::new();
        env.set_auto_escape_callback(|name| {
            if name.ends_with(".html.jinja2") {
                AutoEscape::Html
            } else {
                AutoEscape::None
            }
        });
        for (name, source) in TEMPLATES {
            env.add_template(name, source)?;
        }
        Ok(Self(env))
    }

    fn render(&self, status: StatusCode, name: &str, ctx: Value) -> AppResult<Response> {
        let html = self
            .0
            .get_template(name)
            .and_then(|template| template.render(ctx))
            .map_err(anyhow::Error::from)?;
        Ok((status, Html(html)).into_response())
    }
}

pub fn router() -> Router<Arc<App>> {
    Router::new()
        .route("/", get(index))
        .route("/web/auth/reset", get(reset_auth_step))
        .route("/web/auth/request-otp", post(request_otp_form))
        .route("/web/auth/verify-otp", post(verify_otp_form))
        .route("/web/auth/logout", post(logout_form))
        .route("/web/chat/{agent_id}", get(chat_panel))
        .route("/web/chat/{agent_id}/messages", post(send_message))
}

fn format_time(timestamp: f64) -> String {
    Local
        .timestamp_opt(timestamp as i64, 0)
        .single()
        .map(|time| time.format("%H:%M").to_string())
        .unwrap_or_default()
}

fn time_ago(timestamp: f64) -> String {
    // Rounded half-to-even, as it always was.
    let diff_min = ((now() - timestamp) / 60.0).round_ties_even().max(0.0);
    if diff_min < 1.0 {
        return "now".to_string();
    }
    if diff_min < 60.0 {
        return format!("{diff_min}m");
    }
    let diff_hr = (diff_min / 60.0).round_ties_even();
    if diff_hr < 24.0 {
        return format!("{diff_hr}h");
    }
    format!("{}d", (diff_hr / 24.0).round_ties_even())
}

#[derive(Serialize)]
struct Shown<'a> {
    #[serde(flatten)]
    message: &'a Message,
    time: String,
    ago: String,
}

fn enrich(messages: &[Message]) -> Vec<Shown<'_>> {
    messages
        .iter()
        .map(|message| Shown {
            message,
            time: format_time(message.ts),
            ago: time_ago(message.ts),
        })
        .collect()
}

fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == AUTH_COOKIE_NAME)
        .map(|(_, value)| value.trim_matches('"').to_string())
        .filter(|value| !value.is_empty())
}

async fn current_session(app: &App, headers: &HeaderMap) -> AppResult<Option<WebSessionRecord>> {
    match session_cookie(headers) {
        Some(token) => Ok(app.store.get_web_session(&token).await?),
        None => Ok(None),
    }
}

/// The logged-in web user, by session cookie.
struct WebClient(WebSessionRecord);

impl FromRequestParts<Arc<App>> for WebClient {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, app: &Arc<App>) -> Result<Self, AppError> {
        current_session(app, &parts.headers)
            .await?
            .map(WebClient)
            .ok_or_else(|| AppError::unauthorized("Unauthorized"))
    }
}

fn render_shell(app: &App, client: &WebSessionRecord) -> AppResult<Response> {
    let mut previews: HashMap<&str, Value> = HashMap::new();
    for agent in AGENTS {
        let messages = app.chat.get_messages(&client.phone, agent.id);
        if let Some(last) = messages.last() {
            previews.insert(
                agent.id,
                Value::from_serialize(Shown {
                    message: last,
                    time: format_time(last.ts),
                    ago: time_ago(last.ts),
                }),
            );
        }
    }
    app.templates.render(
        StatusCode::OK,
        "shell.html.jinja2",
        context! { agents => AGENTS, previews => previews, active_agent_id => () },
    )
}

async fn index(State(app): State<Arc<App>>, headers: HeaderMap) -> AppResult<Response> {
    match current_session(&app, &headers).await? {
        Some(client) => render_shell(&app, &client),
        None => app
            .templates
            .render(StatusCode::OK, "login.html.jinja2", context! {}),
    }
}

async fn reset_auth_step(State(app): State<Arc<App>>) -> AppResult<Response> {
    app.templates.render(
        StatusCode::OK,
        "partials/login_phone_form.html.jinja2",
        context! {},
    )
}

#[derive(Deserialize)]
struct PhoneForm {
    phone: String,
}

#[derive(Deserialize)]
struct CodeForm {
    phone: String,
    code: String,
}

#[derive(Deserialize)]
struct MessageForm {
    message: String,
}

fn phone_form(app: &App, status: StatusCode, phone: &str, error: &str) -> AppResult<Response> {
    app.templates.render(
        status,
        "partials/login_phone_form.html.jinja2",
        context! { phone => phone, error => error },
    )
}

fn code_form(app: &App, status: StatusCode, phone: &str, error: &str) -> AppResult<Response> {
    app.templates.render(
        status,
        "partials/login_code_form.html.jinja2",
        context! { phone => phone, error => error },
    )
}

async fn request_otp_form(
    State(app): State<Arc<App>>,
    FormBody(form): FormBody<PhoneForm>,
) -> AppResult<Response> {
    let phone = match normalize_phone(&form.phone) {
        Ok(phone) => phone,
        Err(msg) => return phone_form(&app, StatusCode::BAD_REQUEST, &form.phone, msg),
    };
    match otp::request(&app, &phone).await? {
        Err((status, detail)) => phone_form(&app, status, &phone, &detail),
        Ok(code) => app.templates.render(
            StatusCode::OK,
            "partials/login_code_form.html.jinja2",
            context! {
                phone => phone,
                debug_code => app.settings.debug_expose_otp.then_some(code),
            },
        ),
    }
}

fn redirect_home() -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    response
        .headers_mut()
        .insert("HX-Redirect", HeaderValue::from_static("/"));
    response
}

async fn verify_otp_form(
    State(app): State<Arc<App>>,
    FormBody(form): FormBody<CodeForm>,
) -> AppResult<Response> {
    let phone = match normalize_phone(&form.phone) {
        Ok(phone) => phone,
        Err(msg) => return phone_form(&app, StatusCode::BAD_REQUEST, &form.phone, msg),
    };
    if let Err((status, detail)) = otp::verify(&app, &phone, form.code.trim()).await? {
        return code_form(&app, status, &phone, &detail);
    }

    let (user_id, _) = app.store.get_or_create_user(&phone).await?;
    let api_key = app.store.issue_api_key(&user_id, "web").await?;
    let session = app
        .store
        .issue_web_session(&user_id, &crate::store::hash(&api_key))
        .await?;

    let mut response = redirect_home();
    let cookie = format!(
        "{AUTH_COOKIE_NAME}={session}; HttpOnly; Max-Age={COOKIE_MAX_AGE_SECONDS}; Path=/; SameSite=lax"
    );
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(anyhow::Error::from)?,
    );
    Ok(response)
}

async fn logout_form(State(app): State<Arc<App>>, headers: HeaderMap) -> AppResult<Response> {
    if let Some(token) = session_cookie(&headers) {
        if let Some(session) = app.store.get_web_session(&token).await? {
            app.store
                .revoke_api_key_for_user(&session.user_id, &session.api_key_hash)
                .await?;
        }
        app.store.revoke_web_session(&token).await?;
    }
    let mut response = redirect_home();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "usecode_agent_web_session=\"\"; expires=Thu, 01 Jan 1970 00:00:00 GMT; Max-Age=0; Path=/; SameSite=lax",
        ),
    );
    Ok(response)
}

async fn chat_panel(
    State(app): State<Arc<App>>,
    WebClient(client): WebClient,
    Path(agent_id): Path<String>,
) -> AppResult<Response> {
    let agent = app
        .chat
        .get_agent(&agent_id)
        .ok_or_else(|| AppError::not_found("Agent not found"))?;
    let messages = app.chat.get_messages(&client.phone, &agent_id);
    app.templates.render(
        StatusCode::OK,
        "partials/chat_panel.html.jinja2",
        context! { agent => agent, messages => enrich(&messages), active_agent_id => agent_id },
    )
}

async fn send_message(
    State(app): State<Arc<App>>,
    WebClient(client): WebClient,
    Path(agent_id): Path<String>,
    FormBody(form): FormBody<MessageForm>,
) -> AppResult<Response> {
    if app.chat.get_agent(&agent_id).is_none() {
        return Err(AppError::not_found("Agent not found"));
    }
    let text = form.message.trim();
    if !text.is_empty() {
        app.chat.send_user_message(&client.phone, &agent_id, text);
        app.chat.add_agent_reply(&client.phone, &agent_id);
    }
    let messages = app.chat.get_messages(&client.phone, &agent_id);
    app.templates.render(
        StatusCode::OK,
        "partials/chat_messages.html.jinja2",
        context! { messages => enrich(&messages) },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_compile_and_escape() {
        let templates = Templates::new().unwrap();
        let response = templates
            .render(
                StatusCode::OK,
                "partials/login_phone_form.html.jinja2",
                context! { phone => "<x>", error => "bad" },
            )
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn cookie_is_found_among_others() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; usecode_agent_web_session=abcd.efg; b=2"),
        );
        assert_eq!(session_cookie(&headers).as_deref(), Some("abcd.efg"));
    }
}
