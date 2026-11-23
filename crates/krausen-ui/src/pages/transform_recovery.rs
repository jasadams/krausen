// SPDX-License-Identifier: AGPL-3.0-or-later
//! The original Scale apply and Duplicate requests, retained from the moment
//! they are dispatched until their result is known (INT-004, INT-008). A record
//! is one finite per-request command, not a draft: closing the dialog, Escape,
//! navigating away or reloading never discards it, and it is replayed verbatim
//! (same request identity and payload) by an explicit retry. Both shared
//! operations recover a repeated `request_id` from their recorded result
//! (`krausen-app` `scaling::apply`/`scaling::duplicate`), so a second tab or a
//! replay after a lost response cannot apply twice or create a second recipe.
//!
//! This follows the `start_recovery.rs`/`library_recovery.rs` convention and is
//! deliberately not shared with them: each retains a different command family
//! with its own payload type and scope (a recipe here, a catalogue entry or a
//! brew start there). They share only `utils::storage::local_storage` and the
//! storage-key convention (prefix, then a JSON scope tuple). Scope is
//! installation, account, workspace and recipe, so an unresolved Scale of one
//! recipe never blocks another.
use krausen_types::{
    brewery::RecipeDocumentV1,
    request_context::OperationIntent,
    scaling::{DuplicateRecipe, ScaleApplyRequest},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum TransformCommand {
    /// The apply request and the exact scaled document the previewed apply
    /// would commit, so a replay after reload can still synchronise the editor.
    Scale { request: ScaleApplyRequest, scaled: RecipeDocumentV1 },
    Duplicate(DuplicateRecipe),
}

impl TransformCommand {
    fn identity(&self) -> (&OperationIntent, &str, &str) {
        match self {
            Self::Scale { request, .. } => (&request.intent, &request.recipe_id, &request.request_id),
            Self::Duplicate(c) => (&c.intent, &c.source_recipe_id, &c.request_id),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Record {
    version: u8,
    installation: String,
    command: TransformCommand,
}

const PREFIX: &str = "krausen-transform-v1:";

fn key(installation: &str, command: &TransformCommand) -> Result<String, String> {
    let (intent, recipe, request) = command.identity();
    serde_json::to_string(&(installation, &intent.account_id, &intent.workspace_id, recipe, request))
        .map(|tuple| format!("{PREFIX}{tuple}"))
        .map_err(|e| e.to_string())
}

#[cfg(any(target_arch = "wasm32", test))]
fn key_scope(key: &str) -> Option<(String, String, String, String, String)> {
    serde_json::from_str(key.strip_prefix(PREFIX)?).ok()
}

/// A stored record is trusted only when its key, scope and payload agree.
/// Anything else fails closed: no new Scale/Duplicate is permitted until the
/// retained original is resolved, rather than guessing or discarding it.
#[cfg(any(target_arch = "wasm32", test))]
fn validate(
    stored_key: &str,
    record: &Record,
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
) -> Result<(), String> {
    let (command_intent, command_recipe, _) = record.command.identity();
    if record.version != 1
        || record.installation != installation
        || command_intent != intent
        || command_recipe != recipe_id
        || key(installation, &record.command).as_deref() != Ok(stored_key)
    {
        return Err("A retained Scale or Duplicate request does not match its scope; no new one is permitted".into());
    }
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Result<web_sys::Storage, String> {
    crate::utils::storage::local_storage("Scale and Duplicate recovery")
}

/// Retain the exact command before it is sent. A different payload already
/// owning the request identity is refused, never replaced.
pub(crate) fn retain(installation: &str, command: &TransformCommand) -> Result<(), String> {
    let key = key(installation, command)?;
    let record = Record { version: 1, installation: installation.to_owned(), command: command.clone() };
    let json = krausen_types::request_context::checked_json(&record)
        .map_err(|_| "The request cannot be retained without changing its numbers".to_owned())?;
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        if let Some(existing) = storage.get_item(&key).map_err(|_| "Unable to read Scale and Duplicate recovery")? {
            let previous: Record = serde_json::from_str(&existing)
                .map_err(|_| "An earlier Scale or Duplicate recovery record is unreadable")?;
            return if previous.command == *command {
                Ok(())
            } else {
                Err("A different payload already owns this request identity".into())
            };
        }
        storage.set_item(&key, &json).map_err(|_| "Unable to retain the original request")?;
    }
    let _ = (key, json);
    Ok(())
}

/// Remove a record after a known outcome only.
pub(crate) fn remove(installation: &str, command: &TransformCommand) {
    #[cfg(target_arch = "wasm32")]
    if let Ok(key) = key(installation, command) {
        if let Ok(storage) = storage() {
            if storage.remove_item(&key).is_err() {
                tracing::warn!("Unable to remove resolved Scale and Duplicate recovery");
            }
        }
    }
    let _ = (installation, command);
}

/// The retained command for exactly this installation/account/workspace/recipe.
pub(crate) fn load(
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
) -> Result<Option<TransformCommand>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        let mut found = Vec::new();
        for index in 0..storage.length().map_err(|_| "Unable to list Scale and Duplicate recovery")? {
            let Some(stored_key) = storage.key(index).map_err(|_| "Unable to read recovery key")? else { continue };
            let Some((stored_installation, account, workspace, recipe, _)) = key_scope(&stored_key) else { continue };
            if stored_installation != installation
                || account != intent.account_id
                || workspace != intent.workspace_id
                || recipe != recipe_id
            {
                continue;
            }
            let Some(json) = storage.get_item(&stored_key).map_err(|_| "Unable to read recovery")? else { continue };
            let record: Record = serde_json::from_str(&json)
                .map_err(|_| "A retained Scale or Duplicate request is unreadable; no new one is permitted")?;
            validate(&stored_key, &record, installation, intent, recipe_id)?;
            found.push((stored_key, record.command));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        return Ok(found.into_iter().next().map(|(_, command)| command));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (installation, intent, recipe_id);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent() -> OperationIntent {
        OperationIntent { account_id: "a".into(), workspace_id: "w".into() }
    }

    fn duplicate(request: &str) -> TransformCommand {
        TransformCommand::Duplicate(DuplicateRecipe {
            intent: intent(),
            source_recipe_id: "r".into(),
            expected_revision: 2,
            title: Some("Copy".into()),
            request_id: request.into(),
        })
    }

    #[test]
    fn a_retained_duplicate_round_trips_with_its_original_identity_and_payload() {
        let command = duplicate("request-1");
        let json = krausen_types::request_context::checked_json(&Record {
            version: 1,
            installation: "http://local".into(),
            command: command.clone(),
        })
        .unwrap();
        let recovered: Record = serde_json::from_str(&json).unwrap();
        assert_eq!(recovered.command, command);
        let stored = key("http://local", &command).unwrap();
        assert_eq!(
            key_scope(&stored),
            Some(("http://local".into(), "a".into(), "w".into(), "r".into(), "request-1".into()))
        );
        assert_ne!(stored, key("http://local", &duplicate("request-2")).unwrap());
        assert_ne!(stored, key("http://other", &command).unwrap());
    }

    #[test]
    fn only_a_record_whose_key_scope_and_payload_agree_is_trusted() {
        let command = duplicate("request-1");
        let stored = key("http://local", &command).unwrap();
        let record = || Record { version: 1, installation: "http://local".into(), command: command.clone() };
        assert!(validate(&stored, &record(), "http://local", &intent(), "r").is_ok());
        let mut wrong_version = record();
        wrong_version.version = 2;
        assert!(validate(&stored, &wrong_version, "http://local", &intent(), "r").is_err());
        assert!(validate(&stored, &record(), "http://other", &intent(), "r").is_err(), "installation");
        assert!(validate(&stored, &record(), "http://local", &intent(), "other-recipe").is_err(), "recipe");
        let other_account = OperationIntent { account_id: "b".into(), workspace_id: "w".into() };
        assert!(validate(&stored, &record(), "http://local", &other_account, "r").is_err(), "account");
        let other_key = key("http://local", &duplicate("request-2")).unwrap();
        assert!(validate(&other_key, &record(), "http://local", &intent(), "r").is_err(), "key/payload mismatch");
    }
}
