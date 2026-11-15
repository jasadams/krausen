// SPDX-License-Identifier: AGPL-3.0-or-later
//! Original library write commands (Save, Archive/Restore) retained until their
//! result is known. This is a finite per-request record, not a draft or a
//! command queue: closing the dialog, navigating away or reloading never
//! discards it, and it is replayed verbatim (same request identity and payload)
//! by an explicit check. Scope is installation, account and workspace. Replay
//! reuses the original `request_id`, which the server treats idempotently, so
//! two tabs that both replay one record cannot apply it twice; unlike evidence
//! writes no tab lock, status or receipt is therefore needed.
use krausen_types::{
    catalogue::{ArchiveIngredientEntry, SaveIngredientEntry},
    request_context::OperationIntent,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum LibraryCommand {
    Save(SaveIngredientEntry),
    Archive(ArchiveIngredientEntry),
}

impl LibraryCommand {
    fn identity(&self) -> (&OperationIntent, &str, &str) {
        match self {
            Self::Save(c) => (&c.intent, &c.entry_id, &c.request_id),
            Self::Archive(c) => (&c.intent, &c.entry_id, &c.request_id),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Record {
    version: u8,
    installation: String,
    command: LibraryCommand,
}

const PREFIX: &str = "krausen-library-v1:";

fn key(installation: &str, command: &LibraryCommand) -> Result<String, String> {
    let (intent, _, request) = command.identity();
    serde_json::to_string(&(installation, &intent.account_id, &intent.workspace_id, request))
        .map(|tuple| format!("{PREFIX}{tuple}"))
        .map_err(|e| e.to_string())
}

#[cfg(any(target_arch = "wasm32", test))]
fn key_scope(key: &str) -> Option<(String, String, String, String)> {
    serde_json::from_str(key.strip_prefix(PREFIX)?).ok()
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Result<web_sys::Storage, String> {
    crate::utils::storage::local_storage("Library recovery")
}

/// Retains the exact command before it is sent. A different payload already
/// owning the request identity is refused, never replaced.
pub(crate) fn retain(installation: &str, command: &LibraryCommand) -> Result<(), String> {
    let key = key(installation, command)?;
    let record = Record { version: 1, installation: installation.to_owned(), command: command.clone() };
    let json = krausen_types::request_context::checked_json(&record)
        .map_err(|_| "The library request cannot be retained without changing its numbers".to_owned())?;
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        if let Some(existing) = storage.get_item(&key).map_err(|_| "Unable to read library recovery")? {
            let previous: Record = serde_json::from_str(&existing)
                .map_err(|_| "An earlier library recovery record is unreadable")?;
            return if previous.command == *command {
                Ok(())
            } else {
                Err("A different payload already owns this library request identity".into())
            };
        }
        storage.set_item(&key, &json).map_err(|_| "Unable to retain the original library request")?;
    }
    let _ = (key, json);
    Ok(())
}

pub(crate) fn remove(installation: &str, command: &LibraryCommand) {
    #[cfg(target_arch = "wasm32")]
    if let Ok(key) = key(installation, command) {
        if let Ok(storage) = storage() {
            if storage.remove_item(&key).is_err() {
                tracing::warn!("Unable to remove resolved library recovery");
            }
        }
    }
    let _ = (installation, command);
}

/// The retained command for exactly this installation/account/workspace, if any.
/// An unreadable or mismatched in-scope record fails closed: no new write.
pub(crate) fn load(installation: &str, intent: &OperationIntent) -> Result<Option<LibraryCommand>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        // Trade-off, recorded deliberately: without storage `retain` refuses
        // every send, so no library write can start here, and failing closed
        // would also block ordinary catalogue Add in private browsing. A record
        // retained earlier while storage worked is therefore not consulted, but
        // nothing new can be dispatched either, so no duplicate can be minted.
        let Ok(storage) = storage() else { return Ok(None) };
        let mut found = Vec::new();
        for index in 0..storage.length().map_err(|_| "Unable to list library recovery")? {
            let Some(key) = storage.key(index).map_err(|_| "Unable to read library recovery key")? else { continue };
            let Some((stored_installation, account, workspace, _)) = key_scope(&key) else { continue };
            if stored_installation != installation || account != intent.account_id || workspace != intent.workspace_id {
                continue;
            }
            let Some(json) = storage.get_item(&key).map_err(|_| "Unable to read library recovery")? else { continue };
            let record: Record = serde_json::from_str(&json)
                .map_err(|_| "A retained library request is unreadable; no new library change is permitted")?;
            if record.version != 1
                || record.installation != installation
                || record.command.identity().0 != intent
                || self::key(installation, &record.command).as_deref() != Ok(key.as_str())
            {
                return Err("A retained library request does not match its scope; no new library change is permitted".into());
            }
            found.push((key, record.command));
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        return Ok(found.into_iter().next().map(|(_, command)| command));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (installation, intent);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use krausen_types::catalogue::IngredientEntryDocumentV1;

    fn save(request: &str) -> LibraryCommand {
        LibraryCommand::Save(SaveIngredientEntry {
            intent: OperationIntent { account_id: "a".into(), workspace_id: "w".into() },
            entry_id: "e".into(),
            document: IngredientEntryDocumentV1::fermentable(
                "Base malt".into(),
                krausen_types::brewery::FermentableSpecification {
                    name: Some("Pale".into()),
                    producer: None,
                    kind: Some("grain".into()),
                    yield_fraction: Some(0.8),
                    colour_ebc: Some(4.5),
                },
            ),
            expected_revision: 0,
            expected_metadata_revision: 0,
            request_id: request.into(),
        })
    }

    #[test]
    fn retained_command_round_trips_with_its_original_identity_and_payload() {
        let command = save("request-1");
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
            Some(("http://local".into(), "a".into(), "w".into(), "request-1".into()))
        );
        assert_ne!(stored, key("http://local", &save("request-2")).unwrap());
        assert_ne!(stored, key("http://other", &command).unwrap());
    }
}
