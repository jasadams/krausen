// SPDX-License-Identifier: AGPL-3.0-or-later

//! TLS provider selection, ported from Kyomi's `tls_provider.rs`. Krausen has no
//! Stripe client, so the "stripe-first" mode is dropped; the other modes are
//! unchanged.

use rustls::crypto::CryptoProvider;

// A fresh subprocess prevents another test's provider initialization from masking
// the Ring + AWS-LC ambiguity, and lets us verify an existing provider separately.
#[test]
fn tls_provider_selection() {
    let Ok(mode) = std::env::var("KRAUSEN_TLS_PROVIDER_TEST") else {
        for mode in ["http-first", "concurrent", "preinstalled"] {
            let output =
                std::process::Command::new(std::env::current_exe().expect("test executable"))
                    .args(["--exact", "tls_provider_selection", "--nocapture"])
                    .env("KRAUSEN_TLS_PROVIDER_TEST", mode)
                    .output()
                    .expect("run isolated provider test");
            assert!(
                output.status.success(),
                "{mode}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr),
            );
        }
        return;
    };

    assert!(CryptoProvider::get_default().is_none());
    // Building a plain Reqwest client must not set a process default.
    reqwest::Client::builder()
        .build()
        .expect("plain client builds");
    assert!(CryptoProvider::get_default().is_none());
    // Both symbols must compile: this regression must keep both backends enabled.
    let ring = rustls::crypto::ring::default_provider();
    let aws_lc = rustls::crypto::aws_lc_rs::default_provider();
    if mode == "preinstalled" {
        aws_lc.install_default().expect("install caller's provider");
    }
    let before = CryptoProvider::get_default().cloned();
    match mode.as_str() {
        "http-first" => {
            krausen_auth::http_client().expect("HTTP factory initializes provider");
            assert!(CryptoProvider::get_default().is_some());
        }
        "concurrent" | "preinstalled" => {}
        _ => panic!("unknown subprocess mode"),
    }
    let barrier = std::sync::Barrier::new(8);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                barrier.wait();
                // No startup/test initializer: exercise the production boundary.
                krausen_auth::http_client().expect("HTTP client builds");
            });
        }
    });
    let after = CryptoProvider::get_default().expect("production installs provider");
    if let Some(before) = before {
        assert!(std::sync::Arc::ptr_eq(&before, after));
    } else {
        assert_eq!(after.cipher_suites, ring.cipher_suites);
    }
}
