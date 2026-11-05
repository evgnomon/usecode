// License-Identifier: HGL
// Copyright (C) The Usecode Authors (see AUTHORS)

//! The TLS crypto provider every HTTPS client here uses, installed once
//! as rustls's process default.
//!
//! graviola is the fast one, but it needs CPU features older machines
//! don't have (on x86_64 AVX2, BMI2 and ADX, so nothing before about 2014,
//! e.g. Ivy Bridge Xeons; on aarch64 the crypto extensions, which e.g. the
//! Raspberry Pi 4 lacks) and panics without them. On such a CPU ring is
//! used instead: production-grade, with portable code that runs anywhere,
//! and its C is built into the static binary.

use std::sync::Once;

/// Install the provider for this CPU as rustls's default, once. Safe to
/// call before every client is built.
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let provider = if graviola_runs_here() {
            rustls_graviola::default_provider()
        } else {
            rustls::crypto::ring::default_provider()
        };
        // Err only when a provider is already installed, which is fine.
        let _ = provider.install_default();
    });
}

/// Whether this CPU has what graviola needs (see its README).
pub fn graviola_runs_here() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("aes")
            && std::arch::is_x86_feature_detected!("ssse3")
            && std::arch::is_x86_feature_detected!("pclmulqdq")
            && std::arch::is_x86_feature_detected!("avx")
            && std::arch::is_x86_feature_detected!("avx2")
            && std::arch::is_x86_feature_detected!("bmi2")
            && std::arch::is_x86_feature_detected!("adx")
    }
    #[cfg(target_arch = "aarch64")]
    {
        std::arch::is_aarch64_feature_detected!("aes")
            && std::arch::is_aarch64_feature_detected!("sha2")
            && std::arch::is_aarch64_feature_detected!("pmull")
            && std::arch::is_aarch64_feature_detected!("neon")
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An HTTPS request with the provider this CPU gets, against a live
    /// host, when UC_TLS_TEST_URL is set; skipped otherwise. Run on an
    /// old CPU, it is the fallback's test.
    #[test]
    fn this_cpus_provider_can_talk_https() {
        let Ok(url) = std::env::var("UC_TLS_TEST_URL") else {
            return;
        };
        install();
        eprintln!("graviola: {}", graviola_runs_here());
        let client = reqwest::blocking::Client::new();
        let response = client.get(&url).send().unwrap();
        eprintln!("{url}: {}", response.status());
    }

    /// The fallback works on any CPU, so check it here too, even where
    /// graviola is the one installed.
    #[test]
    fn the_fallback_builds_a_client_config() {
        let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
        rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap();
    }

    #[test]
    fn installing_twice_is_fine() {
        install();
        install();
    }
}
