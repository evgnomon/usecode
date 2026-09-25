// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! Rustls server configuration requiring client certificates (mutual TLS).

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use axum_server::tls_rustls::RustlsConfig;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};

fn load_certs(path: &Path) -> Result<Vec<CertificateDer<'static>>> {
    CertificateDer::pem_file_iter(path)
        .and_then(|it| it.collect::<Result<Vec<_>, _>>())
        .with_context(|| format!("reading certificates from {}", path.display()))
}

pub fn server_config(cert: &Path, key: &Path, ca: &Path) -> Result<RustlsConfig> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = RootCertStore::empty();
    for c in load_certs(ca)? {
        roots.add(c).context("adding CA certificate")?;
    }
    let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider.clone())
        .build()
        .context("building client certificate verifier")?;
    let key = PrivateKeyDer::from_pem_file(key)
        .with_context(|| format!("reading private key from {}", key.display()))?;
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_client_cert_verifier(verifier)
        .with_single_cert(load_certs(cert)?, key)
        .context("configuring server certificate")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(RustlsConfig::from_config(Arc::new(config)))
}
