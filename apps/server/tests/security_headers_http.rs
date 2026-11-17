// SPDX-License-Identifier: AGPL-3.0-or-later
//! Over plain HTTP no HSTS is advertised (it would only confuse browsers); the
//! other security headers are unchanged. Constants are read once per process, so
//! this lives in its own test binary and sets the installation URL itself.

mod common;

#[tokio::test]
async fn plain_http_installations_do_not_send_hsts() {
    // SAFETY: the only test in this binary; no other thread reads the environment.
    unsafe { std::env::set_var("FRONTEND_URL", "http://localhost:3000") };
    let db = common::sqlite_db().await;
    let srv = common::start(&db).await;
    let response = reqwest::get(format!("{}/api/health", srv.base)).await.unwrap();
    assert!(response.headers().get("strict-transport-security").is_none());
    assert_eq!(response.headers()["x-frame-options"], "DENY");
}
