// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser confirmation of an already authenticated account action.
use crate::state::AppState;
use axum::{
    Form, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use krausen_auth::middleware::AuthUser;
use serde::Deserialize;

#[derive(Deserialize)]
struct ProofInput {
    interaction: String,
    code: String,
}
fn safe(input: &ProofInput) -> bool {
    uuid::Uuid::parse_str(&input.interaction).is_ok()
        && input.code.len() == 43
        && input
            .code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}
/// Render the confirmation page with the production kit's approved tokens.
///
/// This route stays a server-rendered form: the proof token arrives in the URL
/// fragment and is never sent in a request line or referrer. A hydrated Leptos
/// page would require script/wasm/connect sources that weaken the strict CSP,
/// so the form is styled inline with the same token values instead.
fn page(status: StatusCode, body: String) -> Response {
    const STYLE: &str = r#"
:root{--bg:#f7f6f2;--surface:#fffefa;--fg:#292c25;--muted:#6d7166;--border:#dedfd4;--primary:#a34728;--primary-hover:#873a21;--on-primary:#fffdf8;--danger:#a2332c;--focus:#9c5838;--radius:0.5rem;--radius-sm:0.25rem}
@media (prefers-color-scheme:dark){:root{--bg:#1c211c;--surface:#242b24;--fg:#f5f3ec;--muted:#b8b8ad;--border:#3b463b;--primary:#d0805d;--primary-hover:#e0946f;--on-primary:#241a15;--danger:#e8938a;--focus:#d9a184}}
*{box-sizing:border-box}
html{font-size:15px}
body{margin:0;min-height:100vh;display:grid;place-items:center;background:var(--bg);color:var(--fg);font-family:'DM Sans',system-ui,-apple-system,sans-serif;line-height:1.55}
main{width:100%;max-width:32rem;margin:1.5rem;padding:2rem;background:var(--surface);border:1px solid var(--border);border-radius:var(--radius);box-shadow:0 1px 2px rgb(0 0 0 / 4%)}
h1{margin:0 0 .75rem;font-size:1.5rem;line-height:1.25;letter-spacing:-.01em}
p{margin:.5rem 0 1.25rem;color:var(--muted)}
form{margin:0}
button{display:inline-flex;align-items:center;justify-content:center;min-height:2.5rem;padding:0 1rem;border:1px solid transparent;border-radius:var(--radius-sm);background:var(--primary);color:var(--on-primary);font:inherit;font-weight:600;cursor:pointer;transition:background-color 200ms,color 200ms}
button:hover{background:var(--primary-hover)}
button:focus-visible{outline:none;box-shadow:0 0 0 3px color-mix(in srgb,var(--focus) 35%,transparent)}
"#;
    let csp = "default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'";
    (
        status,
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::REFERRER_POLICY, "no-referrer"),
            (header::CONTENT_SECURITY_POLICY, csp),
        ],
        Html(format!(
            "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1,viewport-fit=cover\"><title>Confirm account action</title><style>{STYLE}</style></head><body><main>{body}</main></body></html>"
        )),
    )
        .into_response()
}
async fn show(
    State(state): State<AppState>,
    auth: Result<AuthUser, krausen_core::Error>,
) -> Response {
    if !krausen_app::invitation::delivery_available(true, &state.config.base_url) {
        return page(
            StatusCode::SERVICE_UNAVAILABLE,
            "Confirmation unavailable.".into(),
        );
    }
    if auth.is_err() {
        return page(
            StatusCode::UNAUTHORIZED,
            "Sign in to the account that requested this confirmation, then reopen the email link."
                .into(),
        );
    }
    page(StatusCode::OK, r#"<h1>Review confirmation</h1><p>Loading your request…</p><form id="preview" method="post" action="/account/proof/preview"><input type="hidden" id="interaction" name="interaction"><input type="hidden" id="code" name="code"></form><script>const p=new URLSearchParams(location.hash.slice(1));const i=p.get('interaction')||'',c=p.get('code')||'';history.replaceState(null,'',location.pathname);if(/^[0-9a-f-]{36}$/i.test(i)&&/^[A-Za-z0-9_-]{43}$/.test(c)){document.getElementById('interaction').value=i;document.getElementById('code').value=c;document.getElementById('preview').submit()}</script>"#.into())
}
fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
        .replace('\'', "&#39;")
}

async fn preview(
    State(state): State<AppState>,
    auth: Result<AuthUser, krausen_core::Error>,
    headers: HeaderMap,
    Form(input): Form<ProofInput>,
) -> Response {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| url::Url::parse(v).ok());
    let base = url::Url::parse(&state.config.base_url).ok();
    if !safe(&input)
        || origin
            .zip(base)
            .is_none_or(|(a, b)| a.origin() != b.origin())
    {
        return page(StatusCode::FORBIDDEN, "Confirmation unavailable.".into());
    }
    let Ok(auth) = auth else {
        return page(
            StatusCode::UNAUTHORIZED,
            "Sign in and reopen the confirmation link.".into(),
        );
    };
    let Some(exp) = auth.token_exp else {
        return page(StatusCode::UNAUTHORIZED, "Authentication required.".into());
    };
    let actor = krausen_app::ActorContext {
        actor_id: auth.user_id,
        workspace_id: auth.workspace.workspace_id.unwrap_or_default(),
        credential_id: auth.credential_id,
        access_expires_at: exp,
    };
    let Ok(preview) = krausen_app::invitation::preview(
        &state.db,
        &actor,
        &input.interaction,
        &input.code,
        state.config.jwt_secret.as_bytes(),
    )
    .await
    else {
        return page(
            StatusCode::FORBIDDEN,
            "Confirmation unavailable or expired.".into(),
        );
    };
    let workspace = escape_html(preview.workspace_name.as_deref().unwrap_or(""));
    let description = match preview.action.as_str() {
        "discover" => "Verify your mailbox to review invitations.".to_owned(),
        "accept" => format!("Accept your invitation to {workspace}?"),
        "decline" => format!("Decline your invitation to {workspace}?"),
        "ownership_accept" => format!("Accept ownership of {workspace}?"),
        "ownership_decline" => format!("Decline ownership of {workspace}?"),
        _ => return page(StatusCode::FORBIDDEN, "Confirmation unavailable.".into()),
    };
    page(
        StatusCode::OK,
        format!(
            r#"<h1>Review confirmation</h1><p>{description}</p><form method="post" action="/account/proof"><input type="hidden" name="interaction" value="{}"><input type="hidden" name="code" value="{}"><button type="submit">Confirm</button></form>"#,
            input.interaction, input.code
        ),
    )
}
async fn confirm(
    State(state): State<AppState>,
    auth: Result<AuthUser, krausen_core::Error>,
    headers: HeaderMap,
    Form(input): Form<ProofInput>,
) -> Response {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| url::Url::parse(v).ok());
    let base = url::Url::parse(&state.config.base_url).ok();
    if !safe(&input)
        || origin
            .zip(base)
            .is_none_or(|(a, b)| a.origin() != b.origin())
    {
        return page(StatusCode::FORBIDDEN, "Confirmation unavailable.".into());
    }
    let Ok(auth) = auth else {
        return page(
            StatusCode::UNAUTHORIZED,
            "Sign in and reopen the confirmation link.".into(),
        );
    };
    let Some(exp) = auth.token_exp else {
        return page(StatusCode::UNAUTHORIZED, "Authentication required.".into());
    };
    let actor = krausen_app::ActorContext {
        actor_id: auth.user_id,
        workspace_id: auth.workspace.workspace_id.unwrap_or_default(),
        credential_id: auth.credential_id,
        access_expires_at: exp,
    };
    match krausen_app::invitation::complete(
        &state.db,
        &actor,
        &input.interaction,
        &input.code,
        uuid::Uuid::parse_str(&input.interaction).expect("validated interaction ID"),
        state.config.jwt_secret.as_bytes(),
    )
    .await
    {
        Ok(krausen_types::account::InvitationProofOutcome::MailboxVerified) => page(
            StatusCode::OK,
            format!(
                r#"<h1>Mailbox verified</h1><p><a href="/settings/profile?invitation_discovery={}">Return to invitations</a></p>"#,
                input.interaction
            ),
        ),
        Ok(result) => page(
            StatusCode::OK,
            format!(
                r#"<h1>Response {}</h1><p><a href="/settings/profile">Return to profile</a></p>"#,
                result.as_str()
            ),
        ),
        Err(error) => {
            tracing::warn!(%error, "invitation confirmation failed");
            page(
                StatusCode::FORBIDDEN,
                "Confirmation unavailable or expired.".into(),
            )
        }
    }
}
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/account/proof", get(show).post(confirm))
        .route("/account/proof/preview", axum::routing::post(preview))
}
