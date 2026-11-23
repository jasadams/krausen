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
//! Selection is a pure function over the stored entries (oldest first, an
//! unreadable or mismatched in-scope record or an unidentifiable key fails
//! closed) and an explicit, named discard removes only broken records, never a
//! readable request that may have committed, exactly as `start_recovery.rs` does.
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
    /// Creation time (ms since epoch); orders unresolved records oldest first.
    #[serde(default)]
    created_ms: u64,
}

const PREFIX: &str = "krausen-transform-v1:";

fn key(installation: &str, command: &TransformCommand) -> Result<String, String> {
    let (intent, recipe, request) = command.identity();
    serde_json::to_string(&(installation, &intent.account_id, &intent.workspace_id, recipe, request))
        .map(|tuple| format!("{PREFIX}{tuple}"))
        .map_err(|e| e.to_string())
}

fn key_scope(key: &str) -> Option<(String, String, String, String, String)> {
    serde_json::from_str(key.strip_prefix(PREFIX)?).ok()
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Result<web_sys::Storage, String> {
    crate::utils::storage::local_storage("Scale and Duplicate recovery")
}

#[cfg(target_arch = "wasm32")]
fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}
#[cfg(not(target_arch = "wasm32"))]
fn now_ms() -> u64 {
    0
}

/// Retain the exact command before it is sent. A different payload already
/// owning the request identity is refused, never replaced.
pub(crate) fn retain(installation: &str, command: &TransformCommand) -> Result<(), String> {
    let key = key(installation, command)?;
    let record = Record {
        version: 1,
        installation: installation.to_owned(),
        command: command.clone(),
        created_ms: now_ms(),
    };
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
        match storage() {
            Ok(storage) => {
                if storage.remove_item(&key).is_err() {
                    tracing::warn!("Unable to remove resolved Scale and Duplicate recovery");
                }
            }
            Err(error) => tracing::warn!(error, "Unable to remove resolved Scale and Duplicate recovery"),
        }
    }
    let _ = (installation, command);
}

/// Pure selection over the stored entries (`key`, `value`) so the fail-closed
/// rules are testable without a browser. Only entries whose key names exactly
/// this installation, account, workspace and recipe are considered, so a corrupt
/// record for another recipe never blocks this one. An unreadable or mismatched
/// in-scope record is an error: no new Scale or Duplicate is permitted until it
/// is resolved. Oldest first.
fn select(
    entries: Vec<(String, Option<String>)>,
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
) -> Result<Vec<TransformCommand>, String> {
    let mut found = Vec::new();
    for (stored, value) in entries {
        let Some((i, account, workspace, recipe, _)) = key_scope(&stored) else {
            if stored.starts_with(PREFIX) {
                // The key itself is unreadable, so its recipe cannot be known.
                return Err("A retained Scale or Duplicate request cannot be identified; no new one is permitted".into());
            }
            continue;
        };
        if i != installation || account != intent.account_id || workspace != intent.workspace_id || recipe != recipe_id {
            continue;
        }
        let record: Record = value
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .ok_or("A retained Scale or Duplicate request is unreadable; no new one is permitted")?;
        let (command_intent, command_recipe, _) = record.command.identity();
        if record.version != 1
            || record.installation != installation
            || command_intent != intent
            || command_recipe != recipe_id
            || key(installation, &record.command).as_deref() != Ok(stored.as_str())
        {
            return Err("A retained Scale or Duplicate request does not match its scope; no new one is permitted".into());
        }
        found.push((record.created_ms, stored, record.command));
    }
    found.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    Ok(found.into_iter().map(|(_, _, command)| command).collect())
}

/// Every retained entry (key and raw value; `Err` when the value could not be
/// read). Shared by `load` (which fails closed) and `discard` (which skips).
#[cfg(target_arch = "wasm32")]
fn raw_entries(storage: &web_sys::Storage) -> Result<Vec<(String, Result<Option<String>, ()>)>, String> {
    let mut raw = Vec::new();
    for index in 0..storage.length().map_err(|_| "Unable to list Scale and Duplicate recovery")? {
        let Some(stored) = storage.key(index).map_err(|_| "Unable to read recovery key")? else { continue };
        if stored.starts_with(PREFIX) {
            let value = storage.get_item(&stored).map_err(|_| ());
            raw.push((stored, value));
        }
    }
    Ok(raw)
}

/// Unresolved commands for this installation/account/workspace/recipe, oldest first.
pub(crate) fn load(
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
) -> Result<Vec<TransformCommand>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        // Fails closed: a key whose value cannot be read blocks Scale and Duplicate.
        let entries = raw_entries(&storage)?
            .into_iter()
            .map(|(key, value)| {
                value
                    .map(|value| (key, value))
                    .map_err(|()| "Unable to read Scale and Duplicate recovery".to_owned())
            })
            .collect::<Result<Vec<_>, String>>()?;
        return select(entries, installation, intent, recipe_id);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (installation, intent, recipe_id);
        Ok(Vec::new())
    }
}

/// Keys an explicit discard may remove: only the broken entries (unparseable
/// keys of this family and in-scope entries that are unreadable or do not match
/// their key). A readable request is never listed, because the operation may
/// have committed for it.
fn discardable(
    entries: &[(String, Option<String>)],
    installation: &str,
    intent: &OperationIntent,
    recipe_id: &str,
) -> Vec<String> {
    let mut doomed = Vec::new();
    for (stored, value) in entries {
        let Some((i, account, workspace, recipe, _)) = key_scope(stored) else {
            if stored.starts_with(PREFIX) {
                doomed.push(stored.clone());
            }
            continue;
        };
        if i != installation || account != intent.account_id || workspace != intent.workspace_id || recipe != recipe_id {
            continue;
        }
        if select(vec![(stored.clone(), value.clone())], installation, intent, recipe_id).is_err() {
            doomed.push(stored.clone());
        }
    }
    doomed
}

/// Explicit, named user resolution of unreadable retained records. See [`discardable`].
pub(crate) fn discard_broken(installation: &str, intent: &OperationIntent, recipe_id: &str) {
    #[cfg(target_arch = "wasm32")]
    if let Ok(storage) = storage() {
        let Ok(raw) = raw_entries(&storage) else { return };
        // A key whose value could not be read is neither listed nor deleted: an
        // error is not evidence that the record is broken.
        let entries: Vec<_> = raw.into_iter().filter_map(|(key, value)| value.ok().map(|value| (key, value))).collect();
        for stored in discardable(&entries, installation, intent, recipe_id) {
            if storage.remove_item(&stored).is_err() {
                tracing::warn!("Unable to discard a retained Scale or Duplicate record");
            }
        }
    }
    let _ = (installation, intent, recipe_id);
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
            created_ms: 7,
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

    fn entry(command: &TransformCommand, created_ms: u64) -> (String, Option<String>) {
        let record = Record { version: 1, installation: "http://local".into(), command: command.clone(), created_ms };
        (key("http://local", command).unwrap(), Some(serde_json::to_string(&record).unwrap()))
    }

    #[test]
    fn selection_is_oldest_first_and_scoped_to_one_recipe() {
        let mut other_recipe = duplicate("request-3");
        if let TransformCommand::Duplicate(c) = &mut other_recipe {
            c.source_recipe_id = "elsewhere".into();
        }
        let entries = vec![entry(&duplicate("newer"), 20), entry(&duplicate("older"), 10), entry(&other_recipe, 5)];
        let found = select(entries, "http://local", &intent(), "r").unwrap();
        assert_eq!(found.len(), 2);
        assert!(matches!(&found[0], TransformCommand::Duplicate(c) if c.request_id == "older"));
        assert!(matches!(&found[1], TransformCommand::Duplicate(c) if c.request_id == "newer"));
    }

    #[test]
    fn a_broken_in_scope_record_fails_closed_but_another_recipes_corruption_does_not() {
        let good = entry(&duplicate("request-1"), 1);
        let unreadable = (good.0.clone(), Some("not json".to_owned()));
        assert!(select(vec![unreadable.clone()], "http://local", &intent(), "r").is_err());
        assert!(select(vec![unreadable], "http://local", &intent(), "elsewhere").unwrap().is_empty());
        let mismatched = (key("http://local", &duplicate("request-2")).unwrap(), good.1.clone());
        assert!(select(vec![mismatched], "http://local", &intent(), "r").is_err());
        let unidentifiable = (format!("{PREFIX}garbage"), Some("{}".to_owned()));
        assert!(select(vec![unidentifiable], "http://local", &intent(), "r").is_err(), "an unreadable key could belong to this recipe");
        assert!(select(vec![("unrelated-key".to_owned(), Some("x".to_owned()))], "http://local", &intent(), "r").unwrap().is_empty());
    }

    #[test]
    fn discarding_lists_only_broken_records_never_a_readable_request() {
        let readable = entry(&duplicate("request-1"), 1);
        let broken = (key("http://local", &duplicate("request-2")).unwrap(), Some("not json".to_owned()));
        let unidentifiable = (format!("{PREFIX}garbage"), None);
        let doomed = discardable(&[readable.clone(), broken.clone(), unidentifiable.clone()], "http://local", &intent(), "r");
        assert!(doomed.contains(&broken.0));
        assert!(doomed.contains(&unidentifiable.0));
        assert!(!doomed.contains(&readable.0), "a readable request may have committed");
    }
}
