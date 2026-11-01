// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};

/// The listed token ID is an opaque expected version, not proof of ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRevokeCommand {
    pub expected_current_token_id: String,
    pub request_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SessionRevokeOutcome {
    Revoked {
        token_id: String,
        revoked_current: bool,
    },
    Conflict,
    NotFound,
    Forbidden,
    Invalid,
    RequestIdentityConflict,
    ResultExpired {
        resource_id: Option<String>,
    },
    Retryable,
    Unavailable,
}
