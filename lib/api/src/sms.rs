// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Delivery of login codes: a generic HTTP SMS provider when one is
//! configured, otherwise the log.

use std::time::Duration;

use anyhow::{Context, Result};
use serde_json::json;

use crate::config::Settings;

pub async fn send(settings: &Settings, phone: &str, code: &str) -> Result<()> {
    let (Some(api_url), Some(api_key)) = (&settings.sms_api_url, &settings.sms_api_key) else {
        tracing::info!("SMS to {phone}: your usecode agent login code is {code}");
        return Ok(());
    };
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?
        .post(api_url)
        .bearer_auth(api_key)
        .json(&json!({
            "to": phone,
            "sender": settings.sms_sender_name,
            "message": format!("Your usecode agent login code is {code}"),
        }))
        .send()
        .await
        .context("SMS provider request failed")?
        .error_for_status()
        .context("SMS provider rejected the message")?;
    Ok(())
}
