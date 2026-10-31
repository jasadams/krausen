// SPDX-License-Identifier: AGPL-3.0-or-later
//! Run with FRONTEND_URL set to an HTTPS installation URL.
#[test]
fn https_browser_tokens_are_secure_httponly_and_same_site() {
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
        assert!(value.contains("; SameSite=Strict"));
    }
}
