// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser transport for shared recipe library operations.
use krausen_types::recipe_library::{LibraryCommand, LibraryOutcome};
use leptos::prelude::*;
pub async fn library_command_shared(
    command: LibraryCommand,
) -> Result<LibraryOutcome, ServerFnError> {
    let Ok(json) = krausen_types::request_context::checked_json(&command) else {
        return Ok(LibraryOutcome::Invalid);
    };
    execute_library_command(json).await
}
#[server(prefix = "/leptos-api")]
pub async fn execute_library_command(
    command_json: String,
) -> Result<LibraryOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let Ok(command) = serde_json::from_str(&command_json) else {
        return Ok(LibraryOutcome::Invalid);
    };
    Ok(execute(ac.db(), &actor, command).await)
}
#[cfg(feature = "ssr")]
pub async fn execute(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    command: LibraryCommand,
) -> LibraryOutcome {
    use krausen_app::recipe_library;
    match command {
        LibraryCommand::List(q) => recipe_library::list(db, actor, q).await,
        LibraryCommand::History(q) => recipe_library::history(db, actor, q).await,
        LibraryCommand::Revision(q) => recipe_library::revision(db, actor, q).await,
        LibraryCommand::Archive(q) => recipe_library::archive(db, actor, q).await,
        LibraryCommand::Recover(q) => recipe_library::recover(db, actor, q).await,
    }
}
