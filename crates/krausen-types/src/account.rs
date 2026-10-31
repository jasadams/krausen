// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transport-neutral account data; credentials and security material are excluded.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub theme: String,
}

/// A pending account action requires a real browser reauthentication.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingInteraction {
    pub interaction_id: String,
    pub expires_at: i64,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InteractionStatus {
    pub interaction_id: String,
    pub state: String,
    pub result: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitationSummary {
    pub invitation_id: String,
    pub workspace_id: String,
    pub email: String,
    pub role: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InvitationAction {
    Accept,
    Decline,
}
impl InvitationAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Decline => "decline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvitationProofOutcome {
    MailboxVerified,
    Accepted,
    Declined,
}
impl InvitationProofOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MailboxVerified => "verified",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
        }
    }
}
