// SPDX-License-Identifier: AGPL-3.0-or-later
//! Response headers over real HTTP: every page response carries a per-request
//! CSP nonce that is stamped into the shell, HSTS (https installations) and the
//! configured security headers, the shell is `no-cache`, API responses get the
//! deny-all policy, and CORS is an allow-list with credentials.

mod common;

use reqwest::StatusCode;
use std::sync::Once;

const ORIGIN: &str = "https://app.example.test";
const SHELL: &str = r#"<!doctype html><html><head><link rel="stylesheet" href="/output-abc.css"><script nonce="__KRAUSEN_CSP_NONCE__">window.boot = 1</script><script type="module" nonce="__KRAUSEN_CSP_NONCE__">await 1</script></head><body></body></html>"#;

/// Constants are read once per process, so the installation URL and the built
/// frontend directory are fixed before anything touches them.
fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir = std::env::temp_dir().join(format!("krausen-dist-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("index.html"), SHELL).unwrap();
        // SAFETY: runs once, before any test thread reads the environment.
        unsafe {
            std::env::set_var("FRONTEND_URL", ORIGIN);
            std::env::set_var("TRUNK_DIST_DIR", &dir);
        }
    });
}

fn nonce_of(csp: &str) -> String {
    let script = csp
        .split(';')
        .map(str::trim)
        .find(|d| d.starts_with("script-src "))
        .expect("script-src");
    script
        .split_whitespace()
        .find_map(|t| t.strip_prefix("'nonce-").and_then(|t| t.strip_suffix('\'')))
        .expect("nonce source")
        .to_owned()
}

async fn get(url: &str) -> reqwest::Response {
    reqwest::Client::new().get(url).send().await.unwrap()
}

#[tokio::test]
async fn shell_pages_get_a_fresh_nonce_in_header_and_body() {
    init();
    let db = common::sqlite_db().await;
    let srv = common::start(&db).await;
    // /login (served as the shell), / and /index.html (served from the dist dir).
    let mut nonces = Vec::new();
    for path in ["/login", "/", "/index.html"] {
        let response = get(&format!("{}{}", srv.base, path)).await;
        assert_eq!(response.status(), StatusCode::OK, "{path}");
        let csp = response.headers()["content-security-policy"].to_str().unwrap().to_owned();
        let cache = response.headers()["cache-control"].to_str().unwrap().to_owned();
        let body = response.text().await.unwrap();
        let nonce = nonce_of(&csp);
        assert!(!nonce.is_empty());
        assert!(body.contains(&format!("nonce=\"{nonce}\"")), "{path}: body carries the header's nonce");
        assert!(!body.contains("__KRAUSEN_CSP_NONCE__"), "{path}: placeholder replaced");
        assert_eq!(cache, "no-cache", "{path}");
        assert!(csp.contains("'wasm-unsafe-eval'"), "{csp}");
        assert!(!csp.contains("'unsafe-eval'"), "{csp}");
        let script_src = csp.split(';').find(|d| d.trim().starts_with("script-src")).unwrap();
        assert!(!script_src.contains("'unsafe-inline'"), "{csp}");
        assert!(csp.contains("frame-ancestors 'none'"), "{csp}");
        assert!(csp.contains("font-src 'self'"), "{csp}");
        nonces.push(nonce);
    }
    nonces.sort();
    nonces.dedup();
    assert_eq!(nonces.len(), 3, "each response gets its own nonce");
}

#[tokio::test]
async fn every_response_carries_the_configured_security_headers_and_hsts() {
    init();
    let db = common::sqlite_db().await;
    let srv = common::start(&db).await;
    for path in ["/login", "/api/health"] {
        let response = get(&format!("{}{}", srv.base, path)).await;
        let h = response.headers();
        assert_eq!(h["x-frame-options"], "DENY", "{path}");
        assert_eq!(h["x-content-type-options"], "nosniff", "{path}");
        assert_eq!(h["x-xss-protection"], "1; mode=block", "{path}");
        assert_eq!(h["strict-transport-security"], "max-age=31536000; includeSubDomains", "{path}");
    }
}

#[tokio::test]
async fn api_responses_get_the_deny_all_policy_without_a_nonce() {
    init();
    let db = common::sqlite_db().await;
    let srv = common::start(&db).await;
    let response = get(&format!("{}/api/health", srv.base)).await;
    assert_eq!(
        response.headers()["content-security-policy"],
        "default-src 'none'; frame-ancestors 'none'"
    );
}

#[tokio::test]
async fn cors_is_an_allow_list_with_credentials() {
    init();
    let db = common::sqlite_db().await;
    let srv = common::start(&db).await;
    let preflight = |origin: &'static str| {
        let url = format!("{}/api/v1/auth/refresh", srv.base);
        async move {
            reqwest::Client::new()
                .request(reqwest::Method::OPTIONS, url)
                .header("origin", origin)
                .header("access-control-request-method", "POST")
                .send()
                .await
                .unwrap()
        }
    };
    let allowed = preflight(ORIGIN).await;
    assert_eq!(allowed.headers()["access-control-allow-origin"], ORIGIN);
    assert_eq!(allowed.headers()["access-control-allow-credentials"], "true");
    let denied = preflight("https://evil.example.test").await;
    assert!(
        denied.headers().get("access-control-allow-origin").is_none(),
        "an origin outside the allow-list gets no CORS grant"
    );
    // (tower-http sends allow-credentials on every response; without an
    // allow-origin grant a browser still refuses the cross-origin read.)
}

/// Conditional and range requests for the shell are always answered with a full
/// 200 whose nonce matches its CSP header: a 304 would pair the browser's cached
/// body (stamped with an older nonce) with a fresh header nonce and block every
/// inline script on reload.
#[tokio::test]
async fn conditional_and_range_requests_for_the_shell_get_a_matching_full_response() {
    init();
    let db = common::sqlite_db().await;
    let srv = common::start(&db).await;
    let client = reqwest::Client::new();
    // `/%69ndex.html` is another spelling of the shell that reaches the static
    // file service; it must not offer a validator either.
    for path in ["/", "/index.html", "/%69ndex.html", "/login"] {
        let url = format!("{}{}", srv.base, path);
        let first = client.get(&url).send().await.unwrap();
        assert!(first.headers().get("last-modified").is_none(), "{path}: no validator to revalidate");
        assert!(first.headers().get("etag").is_none(), "{path}: no validator to revalidate");
        let requests = [
            ("If-Modified-Since", "Fri, 01 Jan 2100 00:00:00 GMT"),
            ("If-None-Match", "*"),
            ("Range", "bytes=0-10"),
        ];
        for (name, value) in requests {
            let response = client.get(&url).header(name, value).send().await.unwrap();
            if path == "/%69ndex.html" {
                // Served by the static file service; with no validator ever
                // offered, a browser never sends a conditional request here.
                continue;
            }
            assert_eq!(response.status(), StatusCode::OK, "{path} with {name}");
            let csp = response.headers()["content-security-policy"].to_str().unwrap().to_owned();
            let nonce = nonce_of(&csp);
            let body = response.text().await.unwrap();
            assert!(
                body.contains(&format!("nonce=\"{nonce}\"")),
                "{path} with {name}: body nonce matches the header"
            );
        }
    }
}

#[tokio::test]
async fn mcp_and_api_paths_get_the_deny_all_policy() {
    init();
    let db = common::sqlite_db().await;
    let srv = common::start(&db).await;
    for path in ["/mcp", "/api/health"] {
        let response = get(&format!("{}{}", srv.base, path)).await;
        assert_eq!(
            response.headers()["content-security-policy"],
            "default-src 'none'; frame-ancestors 'none'",
            "{path}"
        );
    }
}
