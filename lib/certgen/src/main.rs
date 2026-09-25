// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

mod cert;
mod cmd;
mod store;

use clap::{Parser, Subcommand};

use crate::store::Store;

/// cert-gen: X.509 certificate management for servers and clients.
///
/// Generates certificates signed by a local CA, suitable for:
/// - nginx and other web servers
/// - Chrome and other browsers (after trusting the CA)
/// - Client authentication
///
/// All certificates are stored in ~/.x509/
#[derive(Parser)]
#[command(name = "certgen", version = "1.0.0", verbatim_doc_comment)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize the Certificate Authority.
    ///
    /// Creates a new CA if one doesn't exist. The CA is used to sign
    /// all server and client certificates.
    Init {
        /// CA validity in days (default: 3650 = 10 years)
        #[arg(long, default_value_t = 3650, allow_negative_numbers = true)]
        days: i64,
        /// RSA key size in bits (default: 4096)
        #[arg(long, default_value_t = 4096, allow_negative_numbers = true)]
        key_size: i64,
        /// Recreate CA even if it exists
        #[arg(long)]
        force: bool,
    },
    /// Create a server certificate.
    ///
    /// NAME: Certificate name (used for filenames)
    ///
    /// Examples:
    ///
    ///     certgen server myapp -d localhost -d myapp.local -i 127.0.0.1
    ///
    ///     certgen server nginx -d example.com -d www.example.com --p12
    #[command(verbatim_doc_comment, about = "Create a server certificate")]
    Server {
        name: String,
        /// Domain name(s) for the certificate
        #[arg(short, long)]
        domain: Vec<String>,
        /// IP address(es) for the certificate
        #[arg(short, long)]
        ip: Vec<String>,
        /// Validity in days (default: 365)
        #[arg(long, default_value_t = 365, allow_negative_numbers = true)]
        days: i64,
        /// RSA key size in bits (default: 2048)
        #[arg(long, default_value_t = 2048, allow_negative_numbers = true)]
        key_size: i64,
        /// Also create PKCS#12 bundle
        #[arg(long, overrides_with = "no_p12")]
        p12: bool,
        /// Don't create PKCS#12 bundle (default)
        #[arg(long, overrides_with = "p12")]
        no_p12: bool,
        /// Password for P12 file
        #[arg(long, default_value = "changeit")]
        p12_password: String,
    },
    /// Create a client certificate.
    ///
    /// NAME: Certificate name (used for filenames and CN)
    ///
    /// Examples:
    ///
    ///     certgen client alice --email alice@example.com
    ///
    ///     certgen client api-client --p12-password mysecret
    #[command(verbatim_doc_comment, about = "Create a client certificate")]
    Client {
        name: String,
        /// Email address for the client certificate
        #[arg(long)]
        email: Option<String>,
        /// Validity in days (default: 365)
        #[arg(long, default_value_t = 365, allow_negative_numbers = true)]
        days: i64,
        /// RSA key size in bits (default: 2048)
        #[arg(long, default_value_t = 2048, allow_negative_numbers = true)]
        key_size: i64,
        /// Create PKCS#12 bundle (default: yes)
        #[arg(long, overrides_with = "no_p12")]
        p12: bool,
        /// Don't create PKCS#12 bundle
        #[arg(long, overrides_with = "p12")]
        no_p12: bool,
        /// Password for P12 file
        #[arg(long, default_value = "changeit")]
        p12_password: String,
    },
    /// List all certificates in ~/.x509/
    List {
        /// Show certificate details
        #[arg(short, long)]
        verbose: bool,
    },
    /// Show details of a specific certificate.
    ///
    /// NAME: Certificate name (without extension)
    Show { name: String },
    /// Verify a certificate against the CA.
    ///
    /// NAME: Certificate name (without extension)
    Verify { name: String },
    /// Export a certificate to PKCS#12 format.
    ///
    /// NAME: Certificate name (without extension)
    ///
    /// Creates a .p12 file that can be imported into browsers and applications.
    #[command(name = "export-p12")]
    ExportP12 {
        name: String,
        /// Password for P12 file
        #[arg(long, default_value = "changeit")]
        password: String,
    },
    /// Delete a certificate and its associated files.
    ///
    /// NAME: Certificate name (without extension)
    Delete {
        name: String,
        /// Don't ask for confirmation
        #[arg(long)]
        force: bool,
    },
    /// Generate nginx SSL configuration snippet.
    ///
    /// NAME: Certificate name (without extension)
    #[command(name = "nginx-config")]
    NginxConfig {
        name: String,
        /// Server name for nginx config
        #[arg(long)]
        server_name: Option<String>,
    },
}

fn run(cli: Cli) -> anyhow::Result<i32> {
    let store = Store::from_home();
    match cli.command {
        Command::Init {
            days,
            key_size,
            force,
        } => cmd::init(&store, days, key_size, force).map(|_| 0),
        Command::Server {
            name,
            domain,
            ip,
            days,
            key_size,
            p12,
            no_p12,
            p12_password,
        } => {
            let _ = no_p12;
            cmd::server(
                &store,
                &name,
                domain,
                ip,
                days,
                key_size,
                p12,
                &p12_password,
            )
            .map(|_| 0)
        }
        Command::Client {
            name,
            email,
            days,
            key_size,
            p12,
            no_p12,
            p12_password,
        } => {
            // --p12 defaults to on; only an effective --no-p12 disables it.
            let _ = p12;
            cmd::client(&store, &name, email, days, key_size, !no_p12, &p12_password).map(|_| 0)
        }
        Command::List { verbose } => cmd::list(&store, verbose).map(|_| 0),
        Command::Show { name } => cmd::show(&store, &name),
        Command::Verify { name } => cmd::verify(&store, &name),
        Command::ExportP12 { name, password } => cmd::export(&store, &name, &password),
        Command::Delete { name, force } => cmd::delete(&store, &name, force),
        Command::NginxConfig { name, server_name } => {
            cmd::nginx_config(&store, &name, server_name.as_deref())
        }
    }
}

fn main() {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
    }
}
