// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared result for an authenticated recipient's invitation response.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationAction {
    Accept,
    Decline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InvitationResponseOutcome {
    Accepted,
    Declined,
    NotFound,
    Conflict,
    RequestIdentityConflict,
    ResultExpired { resource_id: Option<String> },
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}
