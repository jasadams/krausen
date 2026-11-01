// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser transport for shared ingredient catalogue operations.
use krausen_types::catalogue::{IngredientCommand, IngredientOutcome};
use leptos::prelude::*;

pub async fn catalogue_command_shared(
    command: IngredientCommand,
) -> Result<IngredientOutcome, ServerFnError> {
    let json = match krausen_types::request_context::checked_json(&command) {
        Ok(json) => json,
        Err(()) => return Ok(IngredientOutcome::Invalid),
    };
    execute_ingredient_command(json).await
}
#[server(prefix = "/leptos-api")]
pub async fn execute_ingredient_command(
    command_json: String,
) -> Result<IngredientOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let command = match serde_json::from_str(&command_json) {
        Ok(command) => command,
        Err(_) => return Ok(IngredientOutcome::Invalid),
    };
    Ok(execute(ac.db(), &actor, command).await)
}
#[cfg(feature = "ssr")]
pub async fn execute(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    command: IngredientCommand,
) -> IngredientOutcome {
    use krausen_app::catalogue;
    match command {
        IngredientCommand::Save(c) => catalogue::project(catalogue::save(db, actor, c).await),
        IngredientCommand::Archive(c) => {
            catalogue::project(catalogue::set_archived(db, actor, c).await)
        }
        IngredientCommand::Get(c) => {
            catalogue::project_get(catalogue::get(db, actor, &c.intent, &c.entry_id).await)
        }
        IngredientCommand::Search(c) => {
            catalogue::project_page(catalogue::search(db, actor, &c).await)
        }
    }
}
