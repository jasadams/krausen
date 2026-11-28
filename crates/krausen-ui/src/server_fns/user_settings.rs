// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser adapters for the shared per-user settings operations.
#[cfg(feature = "ssr")]
use super::AuthenticatedContext;
use krausen_types::user_settings::{UserSettings, UserSettingsOutcome};
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

#[cfg(feature = "ssr")]
fn own_intent(
    actor: &krausen_app::ActorContext,
) -> krausen_types::request_context::OperationIntent {
    krausen_types::request_context::OperationIntent {
        account_id: actor.actor_id.clone(),
        workspace_id: actor.workspace_id.clone(),
    }
}

/// The signed-in user's settings.
#[server(prefix = "/leptos-api")]
pub async fn get_user_settings() -> Result<UserSettingsOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    Ok(krausen_app::user_settings::project(
        krausen_app::user_settings::get(ac.db(), &actor, &own_intent(&actor)).await,
    ))
}

/// Replace the signed-in user's settings; a missing default batch size unsets it.
#[server(prefix = "/leptos-api", input = Json)]
pub async fn save_user_settings(
    settings: UserSettings,
) -> Result<UserSettingsOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    Ok(krausen_app::user_settings::project(
        krausen_app::user_settings::save(ac.db(), &actor, &own_intent(&actor), &settings).await,
    ))
}
