// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser adapters for the shared recipe editor commands.
#[cfg(feature = "ssr")]
use super::AuthenticatedContext;
use krausen_types::{
    brewery::RecipeDocumentV1,
    recipe_editor::{RecipeReadOutcome, RecipeSaveOutcome},
};
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

#[server(prefix = "/leptos-api")]
pub async fn read_recipe(recipe_id: String) -> Result<RecipeReadOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    Ok(krausen_app::recipe_editor::read(ac.db(), &actor, &recipe_id).await)
}

#[server(prefix = "/leptos-api", input = Json)]
pub async fn save_recipe(
    recipe_id: Option<String>,
    expected_revision: Option<i64>,
    document: RecipeDocumentV1,
    request_id: String,
) -> Result<RecipeSaveOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(request_id) = uuid::Uuid::parse_str(&request_id) else {
        return Ok(RecipeSaveOutcome::InvalidRequest);
    };
    Ok(krausen_app::recipe_editor::save(
        ac.db(),
        &actor,
        recipe_id.as_deref(),
        expected_revision,
        &document,
        request_id,
    )
    .await)
}
