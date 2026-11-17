// SPDX-License-Identifier: AGPL-3.0-or-later
//! Constants are read once per process, so this test sets an HTTPS installation
//! URL itself before the first read.
#[test]
fn https_browser_tokens_are_secure_httponly_and_same_site() {
    // SAFETY: the only test in this binary; no other thread reads the environment.
    unsafe { std::env::set_var("FRONTEND_URL", "https://example.test") };
    let mut headers = axum::http::HeaderMap::new();
    krausen_auth::cookies::set_token_cookies(&mut headers, Some("access"), Some("refresh"));
    let values: Vec<_> = headers
        .get_all(axum::http::header::SET_COOKIE)
        .iter()
        .collect();
    assert_eq!(values.len(), 2);
    for value in values {
        let value = value.to_str().unwrap();
        assert!(value.contains("; Secure"));
        assert!(value.contains("; HttpOnly"));
        assert!(value.to_ascii_lowercase().contains("; samesite=strict"));
    }
}
