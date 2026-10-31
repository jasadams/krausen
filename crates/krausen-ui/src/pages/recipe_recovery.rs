// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser-local drafts are recovery data, never saved recipe history.
use super::recipe_draft::DraftContents;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryScope {
    pub installation: String,
    pub account_id: String,
    pub workspace_id: String,
    pub recipe_id: Option<String>,
    pub base_revision: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecoveryRecord {
    pub version: u8,
    pub scope: RecoveryScope,
    pub draft: DraftContents,
    pub generation: u64,
    pub pending: Option<super::recipe_editor::PendingRecipeSave>,
    pub updated_at: f64,
}

impl RecoveryScope {
    pub fn prefix(&self) -> String {
        // JSON tuple encoding prevents ambiguous separators in user-provided IDs.
        format!(
            "krausen-draft-v1:{}:",
            serde_json::to_string(&(
                &self.installation,
                &self.account_id,
                &self.workspace_id,
                &self.recipe_id
            ))
            .expect("scope contains only strings")
        )
    }
    pub fn key(&self, tab: &str) -> String {
        format!(
            "{}{}:{}",
            self.prefix(),
            self.base_revision
                .map(|n| n.to_string())
                .unwrap_or_else(|| "new".into()),
            tab
        )
    }
    pub fn same_resource(&self, other: &Self) -> bool {
        self.installation == other.installation
            && self.account_id == other.account_id
            && self.workspace_id == other.workspace_id
            && self.recipe_id == other.recipe_id
    }
}

#[cfg(target_arch = "wasm32")]
pub fn find(scope: &RecoveryScope) -> Result<Vec<(String, RecoveryRecord)>, String> {
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("Local storage unavailable")?;
    let mut records = Vec::new();
    let length = storage
        .length()
        .map_err(|_| "Unable to read local drafts")?;
    for index in 0..length {
        let Some(key) = storage
            .key(index)
            .map_err(|_| "Unable to read local drafts")?
        else {
            continue;
        };
        if !key.starts_with(&scope.prefix()) {
            continue;
        }
        let Some(raw) = storage
            .get_item(&key)
            .map_err(|_| "Unable to read local drafts")?
        else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<RecoveryRecord>(&raw) else {
            continue;
        };
        if record.version != 1 || !scope.same_resource(&record.scope) {
            continue;
        }
        records.push((key, record));
    }
    records.sort_by(|a, b| b.1.updated_at.total_cmp(&a.1.updated_at));
    Ok(records)
}

#[cfg(target_arch = "wasm32")]
pub fn write(key: &str, record: &RecoveryRecord) -> Result<(), String> {
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("Local storage unavailable")?;
    let json = serde_json::to_string(record).map_err(|_| "Unable to encode local draft")?;
    storage
        .set_item(key, &json)
        .map_err(|_| "Unable to retain local draft".into())
}

#[cfg(target_arch = "wasm32")]
pub fn remove(key: &str) -> Result<(), String> {
    let storage = web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .ok_or("Local storage unavailable")?;
    storage
        .remove_item(key)
        .map_err(|_| "Unable to remove local draft".into())
}

#[cfg(test)]
mod tests {
    use super::RecoveryScope;
    #[test]
    fn keys_bind_installation_account_workspace_recipe_and_revision() {
        let scope = RecoveryScope {
            installation: "https://brew.example".into(),
            account_id: "a".into(),
            workspace_id: "b".into(),
            recipe_id: Some("r".into()),
            base_revision: Some(7),
        };
        let mut other = scope.clone();
        other.base_revision = Some(8);
        assert!(scope.same_resource(&other));
        assert_ne!(scope.key("tab"), other.key("tab"));
        other.workspace_id = "other".into();
        assert!(!scope.same_resource(&other));
        assert_ne!(scope.prefix(), other.prefix());
        other = scope.clone();
        other.account_id = "other".into();
        assert!(!scope.same_resource(&other));
        other = scope.clone();
        other.installation = "https://elsewhere.example".into();
        assert!(!scope.same_resource(&other));
    }
}
