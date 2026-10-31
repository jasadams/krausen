// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser adapters for shared BeerXML operations.
#[cfg(feature = "ssr")]
use super::AuthenticatedContext;
use krausen_types::interchange::{
    ExportOutcome, ExportRequest, ImportCommitOutcome, ImportPreviewOutcome, ImportRequest,
    OriginalOutcome,
};
use leptos::prelude::*;
use leptos::server_fn::codec::Json;

#[server(prefix = "/leptos-api", input = Json)]
pub async fn preview_beerxml(
    request: ImportRequest,
) -> Result<ImportPreviewOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    Ok(krausen_app::interchange::preview(ac.db(), &actor, &request).await)
}

#[server(prefix = "/leptos-api", input = Json)]
pub async fn commit_beerxml(
    request: ImportRequest,
    request_id: String,
) -> Result<ImportCommitOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(id) = uuid::Uuid::parse_str(&request_id) else {
        return Ok(ImportCommitOutcome::InvalidRequest);
    };
    Ok(krausen_app::interchange::commit(ac.db(), &actor, &request, id).await)
}

#[server(prefix = "/leptos-api", input = Json)]
pub async fn export_beerxml(request: ExportRequest) -> Result<ExportOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    Ok(krausen_app::interchange::export(ac.db(), &actor, &request).await)
}

#[server(prefix = "/leptos-api", input = Json)]
pub async fn original_beerxml(
    intent: krausen_types::request_context::OperationIntent,
    recipe_id: String,
) -> Result<OriginalOutcome, ServerFnError> {
    let ac = AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    Ok(krausen_app::interchange::original(ac.db(), &actor, &intent, &recipe_id).await)
}
