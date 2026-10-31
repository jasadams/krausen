// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transport-neutral account data; credentials and security material are excluded.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Profile {
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub theme: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "field", rename_all = "snake_case")]
pub enum ProfileChange {
    Name { name: String },
    Theme { theme: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileSaveOutcome {
    Saved(Profile),
    Conflict { expected: i64, actual: i64 },
    RequestIdentityConflict,
    ResultExpired,
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}

/// An active workspace membership visible to its account holder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceMembership {
    pub workspace_id: String,
    pub name: Option<String>,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountContext {
    pub profile: Profile,
    pub workspace: Option<WorkspaceMembership>,
    pub is_owner: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub credential_id: String,
    pub kind: String,
    pub revision: i64,
    pub expires_at: i64,
    pub created_at: Option<String>,
    pub last_used: Option<String>,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub country_code: Option<String>,
    pub client_name: Option<String>,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RevokeAllOutcome {
    Revoked { count: i64 },
    Forbidden,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityStatus {
    pub has_password: bool,
    pub totp_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasskeySummary {
    pub credential_id: String,
    pub name: String,
    pub created_at: Option<String>,
    pub last_used: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamMember {
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub role: String,
    pub is_owner: bool,
    pub joined_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSnapshot {
    pub revision: i64,
    pub members: Vec<TeamMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamInvitation {
    pub invitation_id: String,
    pub email: String,
    pub role: String,
    pub status: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipTransfer {
    pub transfer_id: String,
    pub workspace_id: String,
    pub from_user_id: String,
    pub from_user_email: String,
    pub to_user_id: String,
    pub to_user_email: String,
    pub status: String,
    pub created_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipStartResult {
    pub transfer_id: String,
    pub revision: i64,
    pub delivery: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnershipDetail {
    pub transfer_id: String,
    pub workspace_id: String,
    pub workspace_name: String,
    pub from_user_email: String,
    pub expires_at: String,
    pub status: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnershipStartOutcome {
    Queued(OwnershipStartResult),
    Conflict { expected: i64, actual: i64 },
    RequestIdentityConflict,
    ResultExpired,
    Forbidden,
    Invalid,
    Retryable,
    DeliveryUnavailable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnershipResponseOutcome {
    Pending(PendingInteraction),
    Conflict { expected: i64, actual: i64 },
    RequestIdentityConflict,
    ResultExpired,
    Forbidden,
    Invalid,
    Retryable,
    DeliveryUnavailable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TeamChange {
    SetRole { user_id: String, role: String },
    Remove { user_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamChangeOutcome {
    Saved { revision: i64 },
    Conflict { expected: i64, actual: i64 },
    RequestIdentityConflict,
    ResultExpired,
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
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
pub struct ProofPreview {
    pub action: String,
    pub workspace_name: Option<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvitationCreated {
    pub invitation_id: String,
    pub revision: i64,
    pub delivery: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InvitationCreateOutcome {
    Queued(InvitationCreated),
    Conflict { expected: i64, actual: i64 },
    RequestIdentityConflict,
    ResultExpired,
    Forbidden,
    Invalid,
    Retryable,
    DeliveryUnavailable,
    Unavailable,
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
    OwnershipAccepted,
    OwnershipDeclined,
}
impl InvitationProofOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MailboxVerified => "verified",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::OwnershipAccepted => "ownership accepted",
            Self::OwnershipDeclined => "ownership declined",
        }
    }
}
