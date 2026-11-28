// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared per-user settings operation contracts.
use crate::request_context::OperationIntent;
use serde::{Deserialize, Serialize};

/// What a user has chosen once for every recipe they start.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UserSettings {
    /// Litres a new recipe starts with; `None` when the user has set none.
    #[serde(default)]
    pub default_batch_size_l: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct UserSettingsQuery {
    pub intent: OperationIntent,
}

/// Replaces the user's settings: a missing or null value unsets that setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SaveUserSettings {
    pub intent: OperationIntent,
    pub settings: UserSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UserSettingsOutcome {
    Settings(UserSettings),
    ContextChanged,
    Forbidden,
    Invalid,
    Unavailable,
}
