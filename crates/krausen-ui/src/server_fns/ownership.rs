// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser adapters for shared ownership-transfer operations.
use leptos::prelude::*;

pub type OwnershipTransfer = krausen_types::account::OwnershipDetail;

#[server(prefix = "/leptos-api")]
pub async fn get_ownership_transfer(
    transfer_id: String,
) -> Result<Option<OwnershipTransfer>, ServerFnError> {
    let auth = super::extract_auth().await?;
    let ctx = super::extract_context()?;
    let actor = super::verified_account_actor(&auth)?;
    match krausen_app::ownership::get(&ctx.db, &actor, &transfer_id).await {
        Ok(detail) => Ok(Some(detail)),
        Err(krausen_app::Error::Forbidden) => Ok(None),
        Err(_) => Err(ServerFnError::new("Unable to load transfer")),
    }
}

/// Begin a bound mailbox confirmation; a pending result is not ownership.
#[server(prefix = "/leptos-api")]
pub async fn begin_ownership_response(
    transfer_id: String,
    workspace_id: String,
    expected_revision: i64,
    accept: bool,
    request_id: String,
) -> Result<krausen_types::account::OwnershipResponseOutcome, ServerFnError> {
    let auth = super::extract_auth().await?;
    let ctx = super::extract_context()?;
    let mut actor = super::verified_account_actor(&auth)?;
    actor.workspace_id = workspace_id;
    let request = uuid::Uuid::parse_str(&request_id)
        .map_err(|_| ServerFnError::new("Invalid request identity"))?;
    let mail_available = krausen_app::invitation::delivery_available(
        krausen_auth::email_service::EmailService::from_env().is_configured(),
        &ctx.config.base_url,
    );
    Ok(krausen_app::ownership::project_response(
        krausen_app::ownership::begin_response(
            &ctx.db,
            &actor,
            &transfer_id,
            accept,
            expected_revision,
            request,
            mail_available,
        )
        .await,
    ))
}
