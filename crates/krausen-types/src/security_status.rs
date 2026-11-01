// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PasswordStatusOutcome {
    Available { has_password: bool },
    Forbidden,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TotpStatusOutcome {
    Available { enabled: bool },
    Forbidden,
    Retryable,
    Unavailable,
}
