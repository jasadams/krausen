// SPDX-License-Identifier: AGPL-3.0-or-later

//! Server functions for the onboarding flow.
//!
//! Thin wrappers around [`krausen_auth::onboarding_service`] — they extract HTTP
//! context (cookies, headers) and delegate all business logic to the shared
//! service layer.

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use super::extract_context;

/// Result of the terms acceptance flow.
///
/// Typed variants so the UI can pattern-match. Cookies are set via
/// `ResponseOptions` for the `Success` variant.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AcceptTermsResult {
    /// Terms accepted, session created, cookies set.
    Success,
    /// Error — display the message and allow a retry.
    Error { message: String },
}

/// Accept terms of service, completing the signup or re-acceptance flow.
///
/// Public endpoint — no authentication required. The user is identified by a
/// temporary token stored in the KV store during the OAuth callback. All
/// business logic is delegated to [`krausen_auth::onboarding_service::accept_terms`].
#[server(prefix = "/leptos-api")]
pub async fn accept_terms(
    temp_token: String,
    marketing_consent: bool,
) -> Result<AcceptTermsResult, ServerFnError> {
    let ctx = extract_context()?;

    let headers: axum::http::HeaderMap = leptos_axum::extract()
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to extract headers: {e}")))?;

    let kv = ctx.kv()?;

    let encryption_key = ctx
        .encryption_key
        .as_deref()
        .ok_or_else(|| ServerFnError::new("Encryption key not available"))?;

    let device = super::auth::extract_device_info(&headers);

    let outcome = krausen_auth::onboarding_service::accept_terms(
        &ctx.db,
        &kv,
        encryption_key,
        &ctx.config,
        &device,
        &temp_token,
        marketing_consent,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "accept_terms orchestration failed");
        ServerFnError::new("Internal server error")
    })?;

    match outcome {
        krausen_auth::onboarding_service::AcceptTermsOutcome::Success { cookie_headers } => {
            let response_options = expect_context::<leptos_axum::ResponseOptions>();
            for (name, value) in cookie_headers.iter() {
                if name == axum::http::header::SET_COOKIE {
                    response_options.append_header(name.clone(), value.clone());
                }
            }
            Ok(AcceptTermsResult::Success)
        }
        krausen_auth::onboarding_service::AcceptTermsOutcome::InvalidToken => {
            Ok(AcceptTermsResult::Error {
                message: "Invalid or expired token. Please try signing up again.".to_string(),
            })
        }
    }
}
