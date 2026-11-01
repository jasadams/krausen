// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub role: String,
    pub is_owner: bool,
    pub joined_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberPage {
    pub items: Vec<Member>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MemberListOutcome {
    Available { page: MemberPage },
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}
