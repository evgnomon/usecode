// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! `uc mail settings`: what to type into a mail app (Thunderbird, Apple
//! Mail, K-9, mutt, ...) to read and send mail through the mail server.
//! Every port is TLS with the certificate of the mail name, and both IMAP
//! and SMTP log in with the same name and password.

/// The settings for the mail server answering on `host`, for the account
/// `user` (a placeholder when there is none).
pub fn render(host: &str, user: Option<&str>) -> String {
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    let user = user.unwrap_or("your account name in Stalwart");
    format!(
        "\
Incoming mail (IMAP)
  Server          {host}
  Port            993
  Security        SSL/TLS
  Authentication  normal password
  Username        {user}

Outgoing mail (SMTP)
  Server          {host}
  Port            465
  Security        SSL/TLS
  Authentication  normal password
  Username        {user}

If your app has no SSL/TLS for SMTP, use port 587 with STARTTLS.
POP3 is on 995 (SSL/TLS) too, for apps that want it.
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imap_and_smtp_on_the_mail_name_over_tls() {
        let out = render("Mail.Example.com.", Some("ada@example.com"));
        assert!(out.contains("Server          mail.example.com\n  Port            993"));
        assert!(out.contains("Server          mail.example.com\n  Port            465"));
        assert_eq!(out.matches("Username        ada@example.com").count(), 2);
    }
}
