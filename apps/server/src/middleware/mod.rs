// SPDX-License-Identifier: AGPL-3.0-or-later

//! Middleware stack — CORS, security headers, transparent access-token auto-refresh.



pub mod auth_refresh;

pub use auth_refresh::auth_refresh_middleware;

use axum::{
    body::{Body, to_bytes},
    http::{HeaderName, HeaderValue, Method, Request, header},
    middleware::Next,
    response::Response,
};
use tower_http::cors::{AllowHeaders, CorsLayer};

/// Marker replaced in the frontend shell with its per-response CSP nonce.
pub const CSP_NONCE_PLACEHOLDER: &str = "__KRAUSEN_CSP_NONCE__";

/// Nonce shared by the security middleware and any Leptos SSR for one request.
#[derive(Clone)]
pub struct CspNonce(pub leptos::nonce::Nonce);

/// Build the CORS layer from shared constants.
///
/// Origins, methods, and credentials are read from `shared/constants.toml`
/// (the deployment's own `FRONTEND_URL` is added to the allowed origins).
pub fn cors_layer() -> CorsLayer {
    let constants = krausen_core::constants::get();
    let cors = &constants.cors;

    let origins: Vec<HeaderValue> = cors
        .allowed_origins
        .iter()
        .map(|o| {
            o.parse::<HeaderValue>()
                .unwrap_or_else(|_| panic!("invalid CORS origin in constants.toml: {o}"))
        })
        .collect();

    let methods: Vec<Method> = cors
        .allowed_methods
        .iter()
        .map(|m| {
            m.parse::<Method>()
                .unwrap_or_else(|_| panic!("invalid CORS method in constants.toml: {m}"))
        })
        .collect();

    let mut layer = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(methods)
        .allow_headers(AllowHeaders::mirror_request());

    if cors.allow_credentials {
        layer = layer.allow_credentials(true);
    }

    layer
}

/// Security headers middleware.
///
/// Adds defense-in-depth headers to every response. Header values are read
/// from `shared/constants.toml`.
pub async fn security_headers(mut request: Request<Body>, next: Next) -> Response {
    let is_consent_get = request.method() == Method::GET
        && matches!(
            request.uri().path(),
            "/api/v1/oauth/authorize" | "/api/v1/oauth/authorize/continue"
        );
    let path = request.uri().path();
    // `/mcp` exactly (Kyomi matches only the `/mcp/` prefix): Krausen's
    // `NormalizePathLayer` trims the trailing slash before this middleware sees
    // the request, so the endpoint root arrives as `/mcp`. Kyomi's `/connect/`
    // has no counterpart here.
    let is_api = path.starts_with("/api/")
        || path.starts_with("/ws/")
        || path == "/mcp"
        || path.starts_with("/mcp/");

    // Frontend shell scripts share one unpredictable nonce. API and scriptless
    // OAuth consent responses do not need one.
    let csp_nonce = (!is_api && !is_consent_get).then(leptos::nonce::Nonce::new);
    if let Some(nonce) = csp_nonce.as_ref() {
        request.extensions_mut().insert(CspNonce(nonce.clone()));
    }

    let mut response = next.run(request).await;

    // Trunk embeds the shell once at build time. Substitute the request
    // nonce before returning HTML so its inline scripts match the header.
    if let Some(nonce) = csp_nonce.as_ref()
        && response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|value| value.as_bytes().starts_with(b"text/html"))
    {
        // The shell is small; 4 MiB leaves ample room. If this bound is
        // exceeded, return a non-HTML error instead of delivering scripts
        // without a matching nonce.
        let body = std::mem::replace(response.body_mut(), Body::empty());
        match to_bytes(body, 4 * 1024 * 1024).await {
            Ok(bytes) => {
                let html = String::from_utf8_lossy(&bytes).replace(
                    &format!("nonce=\"{CSP_NONCE_PLACEHOLDER}\""),
                    &format!("nonce=\"{nonce}\""),
                );
                *response.body_mut() = Body::from(html);
                // The body changes on every response, so it must never offer a
                // validator: a conditional request would be answered with a
                // bodiless 304 whose header nonce differs from the nonce in the
                // body the browser cached. (The static file service adds
                // `Last-Modified` for any spelling of the shell's path.)
                let headers = response.headers_mut();
                headers.remove(header::CONTENT_LENGTH);
                headers.remove(header::LAST_MODIFIED);
                headers.remove(header::ETAG);
                headers.remove(header::ACCEPT_RANGES);
            }
            Err(error) => {
                tracing::error!(%error, "frontend HTML exceeded CSP nonce rewrite limit");
                *response.status_mut() = axum::http::StatusCode::INTERNAL_SERVER_ERROR;
                *response.body_mut() = Body::from("Unable to prepare frontend security policy");
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("text/plain; charset=utf-8"),
                );
                response.headers_mut().remove(header::CONTENT_LENGTH);
            }
        }
    }

    let sh = &krausen_core::constants::get().security_headers;
    let headers = response.headers_mut();

    // These are read at every request from the global singleton (cheap — just pointer derefs).
    // Using HeaderValue::from_str because the values come from the TOML file, not static strings.
    if let Ok(v) = HeaderValue::from_str(&sh.x_frame_options) {
        headers.insert(HeaderName::from_static("x-frame-options"), v);
    }
    if let Ok(v) = HeaderValue::from_str(&sh.x_content_type_options) {
        headers.insert(HeaderName::from_static("x-content-type-options"), v);
    }
    if let Ok(v) = HeaderValue::from_str(&sh.x_xss_protection) {
        headers.insert(HeaderName::from_static("x-xss-protection"), v);
    }

    if !sh.hsts.is_empty()
        && let Ok(v) = HeaderValue::from_str(&sh.hsts)
    {
        headers.insert(HeaderName::from_static("strict-transport-security"), v);
    }

    // Consent pages get a scriptless policy (fonts are served from this origin,
    // so font-src is 'self'), API routes use their configured deny-all policy,
    // and frontend routes use nonce-bound scripts.
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
    } else if is_api {
        if let Ok(v) = HeaderValue::from_str(&sh.content_security_policy) {
            headers.insert(HeaderName::from_static("content-security-policy"), v);
        }
    } else {
        let nonce = csp_nonce
            .as_ref()
            .expect("frontend requests receive a CSP nonce");

        headers.insert(
            HeaderName::from_static("content-security-policy"),
            frontend_content_security_policy(nonce),
        );
    }

    response
}

/// Requirements measured from the Trunk shell and the WASM output.
/// `wasm-unsafe-eval` is needed by the Rust/WASM hydration runtime. Styles stay
/// inline for the shell's critical CSS and the runtime `style=` attributes.
/// Fonts, scripts and images are served from this origin; Kyomi's third-party
/// payment and analytics hosts are not allowed because Krausen has none.
fn frontend_content_security_policy(nonce: &leptos::nonce::Nonce) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "default-src 'self'; \
         script-src 'self' 'nonce-{nonce}' 'wasm-unsafe-eval'; \
         style-src 'self' 'unsafe-inline'; \
         font-src 'self'; \
         img-src 'self' data: blob:; \
         connect-src 'self' ws: wss:; \
         worker-src 'self' blob:; \
         manifest-src 'self'; \
         frame-ancestors 'none'"
    ))
    .expect("CSP nonce is a valid header value")
}

#[cfg(test)]
mod tests {
    use super::frontend_content_security_policy;

    #[test]
    fn frontend_csp_limits_scripts_to_nonce_and_measured_dependencies() {
        let nonce = leptos::nonce::Nonce::new();
        let policy = frontend_content_security_policy(&nonce);
        let policy = policy.to_str().expect("CSP should be valid ASCII");
        let script_src = policy
            .split(';')
            .find(|directive| directive.trim_start().starts_with("script-src "))
            .expect("frontend CSP should define script-src");

        assert!(script_src.contains(&format!("'nonce-{nonce}'")));
        assert!(script_src.contains("'wasm-unsafe-eval'"));
        assert!(!script_src.contains("'unsafe-inline'"));
        assert!(!script_src.contains("'unsafe-eval'"));
    }
}
