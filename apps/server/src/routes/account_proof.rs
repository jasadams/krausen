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
fn page(status: StatusCode, body: String) -> Response {
    (status, [(header::CACHE_CONTROL, "no-store"), (header::REFERRER_POLICY, "no-referrer"), (header::CONTENT_SECURITY_POLICY, "default-src 'none'; script-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'")], Html(format!("<!doctype html><html><head><meta charset=\"utf-8\"><title>Confirm account action</title></head><body><main>{body}</main></body></html>"))).into_response()
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
