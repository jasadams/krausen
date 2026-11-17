// SPDX-License-Identifier: AGPL-3.0-or-later
//! Over plain HTTP the browser would reject `Secure` cookies, so only that flag
//! is dropped: SameSite stays Strict and HttpOnly stays set. Constants are read
//! once per process, so this lives in its own test binary and sets the
//! installation URL itself before the first read.
#[test]
fn http_browser_tokens_drop_secure_but_stay_strict_and_httponly() {
    // SAFETY: the only test in this binary; no other thread reads the environment.
    unsafe { std::env::set_var("FRONTEND_URL", "http://localhost:3000") };
    let mut headers = axum::http::HeaderMap::new();
    krausen_auth::cookies::set_token_cookies(&mut headers, Some("access"), Some("refresh"));
    krausen_auth::cookies::clear_token_cookies(&mut headers);
    let values: Vec<_> = headers
        .get_all(axum::http::header::SET_COOKIE)
        .iter()
        .collect();
    // 2 set + 2 clear.
    assert_eq!(values.len(), 4);
    for value in values {
        let value = value.to_str().unwrap();
        assert!(!value.contains("Secure"), "{value}");
        assert!(value.contains("; HttpOnly"), "{value}");
        assert!(value.to_ascii_lowercase().contains("; samesite=strict"), "{value}");
    }
}
