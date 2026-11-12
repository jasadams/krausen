// SPDX-License-Identifier: AGPL-3.0-or-later
//! Original, finite evidence write commands. Not drafts and not a command bus.
use super::{MeasurementCommand, NoteCommand, PendingEvidence};
use crate::components::layout::GuardScope;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub(super) enum EvidenceReceipt {
    Measurement(krausen_types::brew::MeasurementOutcome),
    Note(krausen_types::brew::NoteOutcome),
    Lifecycle(krausen_types::brew::BrewLifecycleOutcome),
    Archive(krausen_types::brew::BrewArchiveOutcome),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum RecoveryStatus {
    #[default]
    Unknown,
    Expired {
        resource_id: Option<String>,
    },
    IdentityConflict,
    ReviewedExpired {
        resource_id: Option<String>,
    },
    ReviewedIdentityConflict,
    Confirmed {
        accepted: bool,
        message: String,
    },
}
impl RecoveryStatus {
    pub fn message(&self) -> String {
        match self {
            Self::Unknown => "The original request may already have committed. Check the same request to confirm its result.".into(),
            Self::Expired { resource_id } => super::expired_result_message(resource_id),
            Self::IdentityConflict => "The original request identity conflicts with different content. It remains retained; verify the existing evidence instead of resending or replacing it.".into(),
            Self::ReviewedExpired { resource_id } => format!("Review acknowledged. {} The original identity remains retained; any further evidence action must be chosen separately.", super::expired_result_message(resource_id)),
            Self::ReviewedIdentityConflict => "Identity-conflict review acknowledged. The original command remains retained and cannot be resent; any further evidence action must be chosen separately.".into(),
            Self::Confirmed { message, .. } => format!("Result confirmed: {message} Only local recovery cleanup remains; no command needs resending."),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct EvidenceRecovery {
    pub version: u8,
    pub installation: String,
    pub command: PendingEvidence,
    pub task_session: String,
    pub generation: u64,
    #[serde(default)]
    pub status: RecoveryStatus,
    #[serde(default)]
    pub receipt: Option<EvidenceReceipt>,
}

impl PendingEvidence {
    pub(super) fn identity(
        &self,
    ) -> Option<(&krausen_types::request_context::OperationIntent, &str, &str)> {
        match self {
            Self::Measurement(command) => match command {
                MeasurementCommand::Record(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
                MeasurementCommand::Correct(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
                MeasurementCommand::Retract(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
                MeasurementCommand::Select(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
                MeasurementCommand::List(_) => None,
            },
            Self::Note(command) => match command {
                NoteCommand::Record(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
                NoteCommand::Correct(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
                NoteCommand::Retract(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
                _ => None,
            },
            Self::Lifecycle(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
            Self::Archive(r) => Some((&r.intent, &r.brew_id, &r.request_id)),
        }
    }
}
impl EvidenceRecovery {
    pub fn matches(&self, scope: &GuardScope) -> bool {
        self.version == 1
            && !self.task_session.is_empty()
            && self.installation == scope.installation
            && self
                .command
                .identity()
                .is_some_and(|(intent, brew, request)| {
                    intent.account_id == scope.account_id
                        && intent.workspace_id == scope.workspace_id
                        && scope.resource_id.as_deref() == Some(brew)
                        && !request.is_empty()
                })
    }
    pub fn key(&self) -> Result<String, String> {
        let (intent, brew, request) = self
            .command
            .identity()
            .ok_or("Only evidence writes can be retained")?;
        serde_json::to_string(&(
            &self.installation,
            &intent.account_id,
            &intent.workspace_id,
            brew,
            request,
        ))
        .map(|tuple| format!("krausen-evidence-v1:{tuple}"))
        .map_err(|e| e.to_string())
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn key_in_scope(key: &str, scope: &GuardScope) -> bool {
    let Some(tuple) = key.strip_prefix("krausen-evidence-v1:") else {
        return false;
    };
    serde_json::from_str::<(String, String, String, String, String)>(tuple)
        .ok()
        .is_some_and(|(installation, account, workspace, brew, request)| {
            installation == scope.installation
                && account == scope.account_id
                && workspace == scope.workspace_id
                && scope.resource_id.as_deref() == Some(brew.as_str())
                && !request.is_empty()
        })
}

pub(super) fn retain(
    scope: &GuardScope,
    command: &PendingEvidence,
    task_session: &str,
    generation: u64,
) -> Result<String, String> {
    let record = EvidenceRecovery {
        version: 1,
        installation: scope.installation.clone(),
        command: command.clone(),
        task_session: task_session.to_owned(),
        generation,
        status: RecoveryStatus::Unknown,
        receipt: None,
    };
    if !record.matches(scope) {
        return Err("Evidence request does not match its original scope".into());
    }
    let key = record.key()?;
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        let value = krausen_types::request_context::checked_json(&record).map_err(
            |_| "Evidence request cannot be retained without changing its numeric payload",
        )?;
        // A second tab must never replace an existing identity with a changed payload.
        if let Some(existing) = storage
            .get_item(&key)
            .map_err(|_| "Unable to read evidence recovery")?
        {
            let previous: EvidenceRecovery = serde_json::from_str(&existing)
                .map_err(|_| "Existing evidence recovery is unreadable")?;
            if !previous.matches(scope)
                || serde_json::to_value(&previous.command).ok()
                    != serde_json::to_value(command).ok()
            {
                return Err("A different payload already owns this request identity".into());
            }
            if previous.status != RecoveryStatus::Unknown {
                return Err(previous.status.message());
            }
            return Ok(key);
        }
        storage
            .set_item(&key, &value)
            .map_err(|_| "Unable to retain the original evidence request")?;
    }
    Ok(key)
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or("Browser unavailable")?
        .local_storage()
        .map_err(|_| "Evidence recovery storage unavailable")?
        .ok_or_else(|| "Evidence recovery storage unavailable".into())
}

pub(super) fn load(scope: &GuardScope) -> Result<Vec<(String, EvidenceRecovery)>, String> {
    #[cfg(target_arch = "wasm32")]
    let mut records = Vec::new();
    #[cfg(not(target_arch = "wasm32"))]
    let records = Vec::new();
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        for index in 0..storage
            .length()
            .map_err(|_| "Unable to list evidence recovery")?
        {
            let Some(key) = storage
                .key(index)
                .map_err(|_| "Unable to read evidence recovery key")?
            else {
                continue;
            };
            if !key_in_scope(&key, scope) {
                continue;
            }
            let Some(json) = storage
                .get_item(&key)
                .map_err(|_| "Unable to read evidence recovery")?
            else {
                continue;
            };
            let record = serde_json::from_str::<EvidenceRecovery>(&json).map_err(|_| "An original scoped evidence record is unreadable; recovery must be reconciled before writing")?;
            if !record.matches(scope) || record.key().ok().as_deref() != Some(&key) {
                return Err("An original scoped evidence record has an unsupported version or mismatched identity; no new write is permitted".into());
            }
            records.push((key, record));
        }
        records.sort_by(|a, b| a.0.cmp(&b.0));
    }
    let _ = scope;
    Ok(records)
}

/// Keeps a Web Lock named for this tab's task session while the page is
/// mounted, so another tab can tell a live owner from an orphaned record.
/// Without Web Locks support every record is treated as orphaned (fail closed).
pub(super) struct SessionLock {
    #[cfg(target_arch = "wasm32")]
    release: Option<js_sys::Function>,
}

#[cfg(target_arch = "wasm32")]
fn lock_name(session: &str) -> String {
    format!("krausen-evidence-session:{session}")
}

#[cfg(target_arch = "wasm32")]
fn lock_manager() -> Option<wasm_bindgen::JsValue> {
    let navigator = web_sys::window()?.navigator();
    let locks = js_sys::Reflect::get(&navigator, &"locks".into()).ok()?;
    (!locks.is_undefined() && !locks.is_null()).then_some(locks)
}

impl SessionLock {
    pub(super) fn acquire(session: &str) -> Self {
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            let release = std::rc::Rc::new(std::cell::RefCell::new(None::<js_sys::Function>));
            let captured = release.clone();
            let held = js_sys::Promise::new(&mut |resolve, _| {
                *captured.borrow_mut() = Some(resolve);
            });
            if let Some(locks) = lock_manager() {
                let callback = wasm_bindgen::closure::Closure::once_into_js(move |_lock: wasm_bindgen::JsValue| held);
                if let Ok(request) = js_sys::Reflect::get(&locks, &"request".into())
                    .and_then(|f| f.dyn_into::<js_sys::Function>())
                {
                    let _ = request.call2(&locks, &lock_name(session).into(), &callback);
                }
            }
            let release = release.borrow_mut().take();
            return Self { release };
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = session;
            Self {}
        }
    }
    pub(super) fn release(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(release) = self.release.take() {
            let _ = release.call0(&wasm_bindgen::JsValue::NULL);
        }
    }
}

/// True when another mounted tab currently owns this record's task session.
/// Such a record is that tab's in-flight request, not an orphan to recover.
pub(super) async fn owned_by_live_tab(record: &EvidenceRecovery, own_session: &str) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        if record.task_session == own_session {
            return false;
        }
        let Some(locks) = lock_manager() else { return false };
        let Ok(query) = js_sys::Reflect::get(&locks, &"query".into())
            .and_then(|f| wasm_bindgen::JsCast::dyn_into::<js_sys::Function>(f))
            .and_then(|f| f.call0(&locks))
        else {
            return false;
        };
        let Ok(state) = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(query)).await else {
            return false;
        };
        let Ok(held) = js_sys::Reflect::get(&state, &"held".into()) else { return false };
        let name = lock_name(&record.task_session);
        return js_sys::Array::from(&held).iter().any(|entry| {
            js_sys::Reflect::get(&entry, &"name".into())
                .ok()
                .and_then(|n| n.as_string())
                .is_some_and(|n| n == name)
        });
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (record, own_session);
        false
    }
}

pub(super) fn remove(key: &str) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    storage()?
        .remove_item(key)
        .map_err(|_| "Unable to remove resolved evidence recovery")?;
    let _ = key;
    Ok(())
}

pub(super) fn load_selected(
    scope: &GuardScope,
    key: &str,
) -> Result<Option<EvidenceRecovery>, String> {
    #[cfg(target_arch = "wasm32")]
    {
        if !key_in_scope(key, scope) {
            return Err("Recovery selection is outside the original scope".into());
        }
        let Some(json) = storage()?
            .get_item(key)
            .map_err(|_| "Unable to read selected evidence recovery")?
        else {
            return Ok(None);
        };
        let record: EvidenceRecovery =
            serde_json::from_str(&json).map_err(|_| "Selected evidence recovery is unreadable")?;
        if !record.matches(scope) || record.key()?.as_str() != key {
            return Err("Selected evidence recovery identity or version changed".into());
        }
        Ok(Some(record))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (scope, key);
        Ok(None)
    }
}

pub(super) fn cleanup_confirmed(
    scope: &GuardScope,
    key: &str,
    status: RecoveryStatus,
) -> Result<(), String> {
    if !matches!(&status, RecoveryStatus::Confirmed { .. }) {
        return Err("Only a known receipt can use cleanup-only reconciliation".into());
    }
    // Another owner may already have removed this exact known record. Missing
    // cleanup is success, not evidence absence or authority to send a command.
    if load_selected(scope, key)?.is_none() {
        return Ok(());
    }
    set_status(key, status)?;
    remove(key)
}

pub(super) fn set_status(key: &str, status: RecoveryStatus) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        let json = storage
            .get_item(key)
            .map_err(|_| "Unable to read original evidence recovery")?
            .ok_or("Original evidence recovery is missing")?;
        let mut record: EvidenceRecovery =
            serde_json::from_str(&json).map_err(|_| "Original evidence recovery is unreadable")?;
        if record.key()?.as_str() != key {
            return Err("Original evidence recovery key changed".into());
        }
        record.status = status.clone();
        storage
            .set_item(
                key,
                &serde_json::to_string(&record).map_err(|e| e.to_string())?,
            )
            .map_err(|_| "Unable to retain the original recovery blocker")?;
    }
    let _ = (key, status);
    Ok(())
}

pub(super) fn set_receipt(
    key: &str,
    receipt: EvidenceReceipt,
    status: RecoveryStatus,
) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let storage = storage()?;
        let json = storage
            .get_item(key)
            .map_err(|_| "Unable to read original recovery")?
            .ok_or("Original recovery is missing")?;
        let mut record: EvidenceRecovery =
            serde_json::from_str(&json).map_err(|_| "Original recovery is unreadable")?;
        if record.key()?.as_str() != key {
            return Err("Original recovery key changed".into());
        }
        record.receipt = Some(receipt.clone());
        record.status = status.clone();
        storage
            .set_item(
                key,
                &serde_json::to_string(&record).map_err(|e| e.to_string())?,
            )
            .map_err(|_| "Unable to retain the original receipt")?;
    }
    let _ = (key, receipt, status);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use krausen_types::{brew::RecordNoteRequest, request_context::OperationIntent};
    #[test]
    fn original_command_roundtrip_and_scope_are_independent_of_unsent_inputs() {
        let scope = GuardScope {
            installation: "http://local".into(),
            account_id: "account:a".into(),
            workspace_id: "workspace:b".into(),
            resource_id: Some("brew:c".into()),
            base_revision: None,
        };
        let command = PendingEvidence::Note(NoteCommand::Record(RecordNoteRequest {
            intent: OperationIntent {
                account_id: scope.account_id.clone(),
                workspace_id: scope.workspace_id.clone(),
            },
            brew_id: "brew:c".into(),
            request_id: "original-id".into(),
            text: "Original semantic payload, not a new field value".into(),
        }));
        let record = EvidenceRecovery {
            version: 1,
            installation: scope.installation.clone(),
            command,
            task_session: "original-session".into(),
            generation: 3,
            status: RecoveryStatus::Unknown,
            receipt: None,
        };
        let json = krausen_types::request_context::checked_json(&record).unwrap();
        let recovered: EvidenceRecovery = serde_json::from_str(&json).unwrap();
        assert!(recovered.matches(&scope));
        assert_eq!(
            serde_json::to_value(&recovered.command).unwrap(),
            serde_json::to_value(&record.command).unwrap()
        );
        assert_eq!(recovered.key().unwrap(), record.key().unwrap());
        assert!(key_in_scope(&recovered.key().unwrap(), &scope));
        assert!(!key_in_scope(
            &recovered.key().unwrap(),
            &GuardScope {
                account_id: "foreign".into(),
                ..scope.clone()
            }
        ));
        assert_eq!(recovered.task_session, "original-session");
        assert_eq!(recovered.generation, 3);
        let mut future = recovered.clone();
        future.version = 99;
        assert!(!future.matches(&scope));
        for different in [
            GuardScope {
                account_id: "other".into(),
                ..scope.clone()
            },
            GuardScope {
                workspace_id: "other".into(),
                ..scope.clone()
            },
            GuardScope {
                resource_id: Some("other".into()),
                ..scope.clone()
            },
            GuardScope {
                installation: "other".into(),
                ..scope.clone()
            },
        ] {
            assert!(!recovered.matches(&different));
        }
    }
    #[test]
    fn read_commands_and_unknown_versions_are_never_recovered_as_writes() {
        let scope = GuardScope {
            installation: "local".into(),
            account_id: "a".into(),
            workspace_id: "w".into(),
            resource_id: Some("b".into()),
            base_revision: None,
        };
        let command = PendingEvidence::Measurement(MeasurementCommand::List(
            krausen_types::brew::MeasurementListRequest {
                intent: OperationIntent {
                    account_id: "a".into(),
                    workspace_id: "w".into(),
                },
                brew_id: "b".into(),
                root_observation_id: None,
                cursor: None,
                limit: 100,
            },
        ));
        let record = EvidenceRecovery {
            version: 1,
            installation: "local".into(),
            command,
            task_session: "original-session".into(),
            generation: 3,
            status: RecoveryStatus::Unknown,
            receipt: None,
        };
        assert!(!record.matches(&scope));
        assert!(record.key().is_err());
    }
}
