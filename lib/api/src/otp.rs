// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The OTP login flow, shared by the JSON API and the web forms.

use anyhow::Result;
use axum::http::StatusCode;
use chrono::{Duration, Utc};
use rand::Rng;

use crate::App;
use crate::chat_store::now;
use crate::sms;

/// A handled failure: the status and message to show the caller.
pub type Refusal = (StatusCode, String);

fn generate_code(length: usize) -> String {
    let mut rng = rand::rng();
    (0..length)
        .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
        .collect()
}

/// Mint a code for `phone` and send it. Returns the code, or a 429 while the
/// previous one is still in its resend cooldown.
pub async fn request(app: &App, phone: &str) -> Result<Result<String, Refusal>> {
    let current = now();
    if let Some(existing) = app.store.get_otp(phone).await?
        && existing.resend_after > current
    {
        let retry_in = (existing.resend_after - current) as i64;
        return Ok(Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!("Please wait {retry_in}s before requesting another code"),
        )));
    }

    let settings = &app.settings;
    let code = generate_code(settings.otp_length);
    let issued = Utc::now();
    app.store
        .put_otp(
            phone,
            &code,
            issued + Duration::seconds(settings.otp_ttl_seconds),
            issued + Duration::seconds(settings.otp_resend_cooldown_seconds),
        )
        .await?;
    sms::send(settings, phone, &code).await?;
    Ok(Ok(code))
}

/// Check `code` against the pending one for `phone`, consuming it on
/// success.
pub async fn verify(app: &App, phone: &str, code: &str) -> Result<Result<(), Refusal>> {
    let refuse = |status, detail: &str| Ok(Err((status, detail.to_string())));
    let Some(record) = app.store.get_otp(phone).await? else {
        return refuse(StatusCode::BAD_REQUEST, "Request a code first");
    };
    if record.expires_at < now() {
        app.store.clear_otp(phone).await?;
        return refuse(StatusCode::BAD_REQUEST, "Code expired, request a new one");
    }
    if record.attempts >= app.settings.otp_max_attempts {
        app.store.clear_otp(phone).await?;
        return refuse(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many attempts, request a new code",
        );
    }
    let numeric = !code.is_empty() && code.chars().all(|c| c.is_ascii_digit());
    if !numeric || !app.store.check_otp_code(phone, code).await? {
        app.store.increment_attempts(phone).await?;
        return refuse(StatusCode::BAD_REQUEST, "Incorrect code");
    }
    app.store.clear_otp(phone).await?;
    Ok(Ok(()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn codes_are_digits_of_the_configured_length() {
        let code = super::generate_code(6);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }
}
