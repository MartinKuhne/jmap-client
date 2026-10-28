/*
 * Copyright Stalwart Labs LLC See the COPYING
 * file at the top-level directory of this distribution.
 *
 * Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
 * https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
 * <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
 * option. This file may not be copied, modified, or distributed
 * except according to those terms.
 */

//! Process-default TLS crypto provider selection.
//!
//! `reqwest` is used through its provider-neutral `rustls-no-provider` feature so
//! that it never forces a crypto provider onto the consumer. The flip side of that
//! feature is that reqwest will not pick a provider by itself: it reads
//! [`rustls::crypto::CryptoProvider::get_default()`] and panics if nothing has
//! been installed.
//!
//! This crate already exposes `ring` and `aws_lc_rs` features, so it owns the job
//! of installing the provider those features select. Installation is one-shot and
//! process-wide, and never overwrites a provider the embedder installed first.

/// Installs the crypto provider selected by this crate's features.
///
/// * `ring` alone installs the *ring* provider.
/// * `aws_lc_rs` installs the aws-lc-rs provider (it wins if both are enabled,
///   which matches the crate default).
/// * With neither feature enabled nothing is installed, and the embedder is
///   expected to call
///   [`rustls::crypto::CryptoProvider::install_default()`] themselves.
///
/// Idempotent, and a no-op when a provider has already been installed.
pub(crate) fn ensure_crypto_provider() {
    static INSTALL: std::sync::Once = std::sync::Once::new();

    INSTALL.call_once(|| {
        // An already-installed provider (one the embedder chose) takes precedence,
        // so a failure to install here is expected and harmless.
        #[cfg(all(feature = "ring", not(feature = "aws_lc_rs")))]
        let _ = rustls::crypto::ring::default_provider().install_default();

        #[cfg(feature = "aws_lc_rs")]
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}
