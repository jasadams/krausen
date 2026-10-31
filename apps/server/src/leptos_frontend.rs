// SPDX-License-Identifier: AGPL-3.0-or-later

//! Leptos frontend serving.
//!
//! Serves the Trunk-built SPA (index.html + WASM bundle + CSS) and
//! falls back to index.html for client-side routing.

use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use tower::ServiceExt;
use tower_http::services::ServeDir;

/// Path to Trunk dist directory (relative to where the binary runs).
/// Set via TRUNK_DIST_DIR env var, or defaults to crates/krausen-ui/dist.
fn dist_dir() -> String {
    std::env::var("TRUNK_DIST_DIR").unwrap_or_else(|_| "crates/krausen-ui/dist".to_string())
}

/// Build a ServeDir service for static assets from trunk dist.
pub fn static_files_service() -> ServeDir {
    ServeDir::new(dist_dir())
}

/// Serve the SPA index.html from trunk dist (fallback for all routes).
pub async fn serve() -> Response {
    let index_path = format!("{}/index.html", dist_dir());
    match tokio::fs::read_to_string(&index_path).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            "Frontend not built. Run: cd crates/krausen-ui && trunk build",
        )
            .into_response(),
    }
}

/// Serve the Leptos shell HTML (for explicit /login, /signup routes).
pub async fn serve_leptos_shell() -> Response {
    serve().await
}

/// Whether a request that matched no file should get the SPA shell.
///
/// The server cannot know which paths the client-side router owns, so it has
/// to guess — and guessing wrong in either direction costs something. Answer
/// the shell too readily and a missing image or script returns HTML with a 200,
/// which hides broken references and makes a typo look like a working page.
/// Answer 404 too readily and real pages report themselves as missing to
/// monitoring, caches and crawlers, which is the bug this whole fallback exists
/// to fix.
///
/// Two signals, either of which is enough:
///
/// - The request accepts HTML. Browsers navigating to a page send
///   `text/html` in `Accept`; a `<script>`, `<img>` or `fetch` for an asset
///   does not. This is the reliable signal when it is present.
/// - The last path segment has no extension. `Accept` is often `*/*` — curl
///   and most uptime checks send exactly that — so extension is what keeps
///   `/settings/profile` a page when nothing states a preference, while
///   `/favicon.ico` and `/app.js` stay missing files.
///
/// A path segment containing a dot is therefore read as a filename, so a route
/// like `/users/jason.smith` relies on the `Accept` signal — which a browser
/// navigating there does send.
fn wants_shell(path: &str, accept: Option<&str>) -> bool {
    if accept.is_some_and(|a| a.contains("text/html")) {
        return true;
    }

    let last_segment = path.rsplit('/').next().unwrap_or_default();
    !last_segment.contains('.')
}

/// Serve a static asset if one matches, otherwise the SPA shell.
///
/// `ServeDir::not_found_service` cannot be used for this: it returns the
/// fallback's body but forces the status to 404, so every client-side route
/// answered "not found" while rendering perfectly. A URL the router handles
/// is a real page and has to say 200 — monitoring, caches and crawlers all
/// read the status rather than the body.
///
/// A request that matches no file and is not a page keeps its 404; see
/// [`wants_shell`].
pub async fn serve_asset_or_shell(request: Request) -> Response {
    let path = request.uri().path().to_string();
    let accept = request
        .headers()
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let response = static_files_service()
        .oneshot(request)
        .await
        .map(IntoResponse::into_response);

    match response {
        Ok(response) if response.status() != StatusCode::NOT_FOUND => response,
        // No such file. A page is handed to the client-side router; anything
        // that reads as a missing asset stays missing.
        _ if wants_shell(&path, accept.as_deref()) => serve().await,
        Ok(response) => response,
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::wants_shell;

    const BROWSER: Option<&str> =
        Some("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8");
    const ASSET: Option<&str> = Some("image/avif,image/webp,*/*");
    const ANYTHING: Option<&str> = Some("*/*");

    #[test]
    fn a_page_a_browser_navigated_to_gets_the_shell() {
        assert!(wants_shell("/settings/profile", BROWSER));
        assert!(wants_shell("/recipes", BROWSER));
        assert!(wants_shell("/", BROWSER));
    }

    #[test]
    fn a_missing_asset_stays_missing() {
        assert!(!wants_shell("/favicon.ico", ASSET));
        assert!(!wants_shell("/app.js", ANYTHING));
        assert!(!wants_shell("/style.css", None));
        assert!(!wants_shell("/bundle.wasm", ANYTHING));
        assert!(!wants_shell("/main.js.map", ANYTHING));
    }

    /// curl and most uptime checks send `*/*`, and they are exactly the
    /// callers this fallback exists for — a page must not read as missing to
    /// them just because they stated no preference.
    #[test]
    fn a_page_is_still_a_page_without_an_accept_preference() {
        assert!(wants_shell("/settings/profile", ANYTHING));
        assert!(wants_shell("/settings/profile", None));
    }

    /// A dotted segment reads as a filename, so this one leans on Accept.
    #[test]
    fn a_route_that_looks_like_a_filename_needs_the_accept_signal() {
        assert!(wants_shell("/users/jason.smith", BROWSER));
        assert!(!wants_shell("/users/jason.smith", ANYTHING));
    }

    #[test]
    fn a_dot_earlier_in_the_path_does_not_make_it_an_asset() {
        assert!(wants_shell("/v1.2/settings", ANYTHING));
    }
}
