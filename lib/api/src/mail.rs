// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Sending mail: the sign up and log in codes, over SMTP.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use mail_send::SmtpClientBuilder;
use mail_send::mail_builder::MessageBuilder;

use crate::config::{Settings, Smtp};

/// A mail as it went out, for [`Mailer::Memory`].
#[derive(Debug, Clone)]
pub struct Sent {
    pub to: String,
    pub subject: String,
    pub text: String,
}

#[derive(Clone)]
pub enum Mailer {
    /// Through an SMTP server, TLS from the first byte on 465, STARTTLS on
    /// other ports.
    Smtp(Smtp),
    /// For trying it out on localhost: the mail goes to the log.
    Log,
    /// Without SMTP on a public console: nothing to send with, so codes
    /// can't go out.
    Off,
    /// Kept in memory, for the tests.
    #[allow(dead_code)]
    Memory(Arc<Mutex<Vec<Sent>>>),
}

impl Mailer {
    pub fn from_settings(settings: &Settings) -> Self {
        match &settings.smtp {
            Some(smtp) => Mailer::Smtp(smtp.clone()),
            None if settings.secure_cookies() => Mailer::Off,
            None => Mailer::Log,
        }
    }

    pub async fn send(&self, to: &str, subject: &str, text: &str) -> Result<()> {
        match self {
            Mailer::Smtp(smtp) => send_smtp(smtp, to, subject, text).await,
            Mailer::Log => {
                tracing::info!("mail to {to}: {subject}\n{text}");
                Ok(())
            }
            Mailer::Off => bail!("mail is off: set USECODE_AGENT_SMTP_PASSWORD"),
            Mailer::Memory(sent) => {
                sent.lock().unwrap().push(Sent {
                    to: to.into(),
                    subject: subject.into(),
                    text: text.into(),
                });
                Ok(())
            }
        }
    }
}

/// `Name <address>`, or a bare address, as `(name, address)`.
fn split_from(from: &str) -> (String, String) {
    match from.split_once('<') {
        Some((name, rest)) => (
            name.trim().trim_matches('"').to_string(),
            rest.trim_end().trim_end_matches('>').trim().to_string(),
        ),
        None => (String::new(), from.trim().to_string()),
    }
}

async fn send_smtp(smtp: &Smtp, to: &str, subject: &str, text: &str) -> Result<()> {
    let (name, address) = split_from(&smtp.from);
    let message = MessageBuilder::new()
        .from((name.as_str(), address.as_str()))
        .to(to)
        .subject(subject)
        .text_body(text);
    let mut client = SmtpClientBuilder::new(smtp.host.as_str(), smtp.port)
        .map_err(anyhow::Error::msg)?
        .implicit_tls(smtp.port == 465)
        .credentials((smtp.user.as_str(), smtp.password.as_str()))
        .timeout(Duration::from_secs(20))
        .connect()
        .await
        .with_context(|| format!("connecting to {}:{}", smtp.host, smtp.port))?;
    client
        .send(message)
        .await
        .with_context(|| format!("sending through {}", smtp.host))?;
    let _ = client.quit().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_from_splits_into_name_and_address() {
        assert_eq!(
            split_from("usecode.dev <no-reply@usecode.dev>"),
            ("usecode.dev".into(), "no-reply@usecode.dev".into())
        );
        assert_eq!(split_from("a@b.co"), (String::new(), "a@b.co".into()));
    }
}
