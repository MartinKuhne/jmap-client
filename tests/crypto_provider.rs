//! Regression tests for issue #34.
//!
//! The crate exposes `ring` and `aws_lc_rs` features that must, on their own, decide
//! which rustls crypto provider the process ends up using.  Two properties are
//! checked here, both without touching the network:
//!
//! 1. `rustls::ClientConfig::builder()` must not panic.  rustls panics when more
//!    than one provider is compiled into the crate because it can no longer tell
//!    which one the caller meant.
//! 2. The process-default provider must be the one the selected feature names.
//!
//! Note: `rustls::crypto::CryptoProvider` has no `name` field in rustls 0.23, so
//! provider identity is established by pointer equality against the `&'static`
//! `secure_random` / `key_provider` singletons that each provider ships.

// These tests are only meaningful when exactly one provider feature is selected.
// Enabling both is ambiguous as far as rustls is concerned, and selecting neither
// leaves the crate without a provider at all; skip the target in both cases.
#![cfg(any(
    all(feature = "ring", not(feature = "aws_lc_rs")),
    all(feature = "aws_lc_rs", not(feature = "ring"))
))]

use std::panic;

use rustls::crypto::CryptoProvider;

/// Two `CryptoProvider`s are the same provider when their `&'static` trait objects
/// are the very same objects.  `default_provider()` builds a fresh struct each call
/// but always points at the same statics, so this is a reliable identity test.
fn is_same(a: &CryptoProvider, b: &CryptoProvider) -> bool {
    std::ptr::eq(a.secure_random, b.secure_random) && std::ptr::eq(a.key_provider, b.key_provider)
}

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).trim().to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.trim().to_string()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Builds a complete, usable `ClientConfig` with an empty root store.  No network.
fn build_client_config() -> Result<rustls::ClientConfig, rustls::Error> {
    Ok(rustls::ClientConfig::builder()
        .with_root_certificates(rustls::RootCertStore::empty())
        .with_no_client_auth())
}

/// `ring` must be the only provider compiled in, so building a rustls
/// `ClientConfig` cannot hit rustls' "ambiguous crypto provider" panic.
#[test]
#[cfg(all(feature = "ring", not(feature = "aws_lc_rs")))]
fn ring_feature_leaves_rustls_client_config_buildable() {
    let built = panic::catch_unwind(build_client_config);
    let built = match built {
        Ok(ok) => ok,
        Err(payload) => panic!(
            "with only the `ring` feature selected, rustls::ClientConfig::builder() must not \
             panic, but it did:\n{}\n\
             This is the issue #34 symptom: reqwest's `rustls` feature force-enables \
             `rustls/aws-lc-rs`, so two providers are compiled in and rustls refuses to choose.",
            panic_text(payload)
        ),
    };
    assert!(built.is_ok(), "client config must build: {built:?}");
}

/// Selecting `ring` must make *ring* the process-default provider, so that every
/// TLS client this crate builds (reqwest included) runs on ring.
#[test]
#[cfg(all(feature = "ring", not(feature = "aws_lc_rs")))]
fn ring_feature_selects_ring_as_process_default() {
    // Creating the crate's client builder is the point at which the crate commits to
    // a provider; no credentials and no network are involved.
    let _builder = jmap_client::client::Client::new();

    let default = CryptoProvider::get_default().unwrap_or_else(|| {
        panic!(
            "`ring` is selected but no process-default crypto provider was installed, so reqwest \
             falls back to whatever provider it was compiled with rather than ring. \
             Expected the process default to be the ring provider."
        )
    });

    let ring = rustls::crypto::ring::default_provider();
    assert!(
        is_same(default, &ring),
        "`ring` is selected but the process-default provider is not ring \
         (default kx_groups={:?}, ring kx_groups={:?})",
        default
            .kx_groups
            .iter()
            .map(|g| g.name())
            .collect::<Vec<_>>(),
        ring.kx_groups.iter().map(|g| g.name()).collect::<Vec<_>>(),
    );
}

/// The reqwest clients this crate builds must come up on the ring provider and must
/// not panic while doing so.
#[test]
#[cfg(all(feature = "ring", not(feature = "aws_lc_rs")))]
fn ring_feature_lets_reqwest_clients_build_on_ring() {
    let _builder = jmap_client::client::Client::new();

    let built = panic::catch_unwind(|| reqwest::Client::builder().build());
    match built {
        Ok(res) => assert!(res.is_ok(), "reqwest client must build: {res:?}"),
        Err(payload) => panic!(
            "building a reqwest client under the `ring` feature panicked:\n{}",
            panic_text(payload)
        ),
    }

    let default =
        CryptoProvider::get_default().expect("reqwest built without a process-default provider");
    let ring = rustls::crypto::ring::default_provider();
    assert!(
        is_same(default, &ring),
        "reqwest must run on the ring provider that the `ring` feature selected"
    );
}

/// Backward compatibility: `aws_lc_rs` keeps working exactly as before.  Gated on
/// `not(feature = "ring")` because enabling both features at once is ambiguous as
/// far as rustls is concerned and is not a configuration this test can satisfy.
#[test]
#[cfg(all(feature = "aws_lc_rs", not(feature = "ring")))]
fn aws_lc_rs_feature_keeps_rustls_client_config_buildable() {
    let built = panic::catch_unwind(build_client_config).unwrap_or_else(|payload| {
        panic!("aws_lc_rs config build panicked: {}", panic_text(payload))
    });
    assert!(built.is_ok(), "client config must build: {built:?}");

    let default = CryptoProvider::get_default().expect("aws_lc_rs must provide a process default");
    let awslc = rustls::crypto::aws_lc_rs::default_provider();
    assert!(
        is_same(default, &awslc),
        "the process-default provider must remain aws-lc-rs when `aws_lc_rs` is selected"
    );
}

/// `aws_lc_rs` must be installed as the process default by the crate itself, so a
/// consumer that only uses the HTTP API gets aws-lc-rs without extra setup.
#[test]
#[cfg(all(feature = "aws_lc_rs", not(feature = "ring")))]
fn aws_lc_rs_feature_selects_aws_lc_rs_as_process_default() {
    let _builder = jmap_client::client::Client::new();

    let default = CryptoProvider::get_default()
        .expect("`aws_lc_rs` is selected and must be installed as the process default");
    let awslc = rustls::crypto::aws_lc_rs::default_provider();
    assert!(
        is_same(default, &awslc),
        "the process-default provider must be aws-lc-rs when `aws_lc_rs` is selected"
    );

    let built = panic::catch_unwind(|| reqwest::Client::builder().build())
        .unwrap_or_else(|payload| panic!("reqwest build panicked: {}", panic_text(payload)));
    assert!(built.is_ok(), "reqwest client must build: {built:?}");
}
