// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser transport for shared equipment operations.
use krausen_types::equipment::{EquipmentCommand, EquipmentOutcome};
use leptos::prelude::*;

pub async fn equipment_command_shared(
    command: EquipmentCommand,
) -> Result<EquipmentOutcome, ServerFnError> {
    let json = match krausen_types::request_context::checked_json(&command) {
        Ok(json) => json,
        Err(()) => return Ok(EquipmentOutcome::Invalid),
    };
    execute_equipment_command(json).await
}
#[server(prefix = "/leptos-api")]
pub async fn execute_equipment_command(
    command_json: String,
) -> Result<EquipmentOutcome, ServerFnError> {
    let ac = super::AuthenticatedContext::extract().await?;
    let actor = super::verified_actor(&ac.auth)?;
    let command = match serde_json::from_str(&command_json) {
        Ok(command) => command,
        Err(_) => return Ok(EquipmentOutcome::Invalid),
    };
    Ok(execute(ac.db(), &actor, command).await)
}
#[cfg(feature = "ssr")]
pub async fn execute(
    db: &krausen_core::DbPool,
    actor: &krausen_app::ActorContext,
    command: EquipmentCommand,
) -> EquipmentOutcome {
    use krausen_app::equipment;
    match command {
        EquipmentCommand::Save(c) => equipment::project(equipment::save(db, actor, c).await),
        EquipmentCommand::Archive(c) => {
            equipment::project(equipment::set_archived(db, actor, c).await)
        }
        EquipmentCommand::Get(c) => {
            equipment::project_get(equipment::get(db, actor, &c.intent, &c.profile_id).await)
        }
        EquipmentCommand::List(c) => equipment::project_list(
            equipment::list(db, actor, &c.intent, &c.after, c.limit, c.include_archived).await,
        ),
        EquipmentCommand::Revision(c) => equipment::project_revision(
            equipment::get_revision(db, actor, &c.intent, &c.profile_id, c.revision).await,
        ),
    }
}
