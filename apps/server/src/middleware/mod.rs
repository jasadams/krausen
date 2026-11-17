// SPDX-License-Identifier: AGPL-3.0-or-later

//! Middleware stack — CORS, security headers, transparent access-token auto-refresh.



use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, Method, Request},
    middleware::Next,
    response::Response,
};
use tower_http::cors::{AllowHeaders, CorsLayer};

/// Build the CORS layer.
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(AllowHeaders::mirror_request())
        .allow_credentials(false)
}

/// Security headers middleware.
pub async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let is_consent_get = request.method() == Method::GET
        && matches!(
            request.uri().path(),
            "/api/v1/oauth/authorize" | "/api/v1/oauth/authorize/continue"
        );

    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        HeaderName::from_static("x-xss-protection"),
        HeaderValue::from_static("1; mode=block"),
    );

    // Consent pages get a scriptless policy. Fonts are served from this origin
    // (/public/fonts, imported by the page stylesheet), so font-src is 'self'.
    if is_consent_get
        && headers
            .get("content-type")
            .is_some_and(|value| value.as_bytes().starts_with(b"text/html"))
    {
        headers.insert(
            HeaderName::from_static("content-security-policy"),
            HeaderValue::from_static(
                "default-src 'none'; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self'; img-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'",
            ),
        );
    }

    response
}
