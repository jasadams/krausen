// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountProfile {
    pub user_id: String,
    pub email: String,
    pub name: Option<String>,
    pub theme: String,
    pub settings_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AccountProfileOutcome {
    Available { profile: AccountProfile },
    Forbidden,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AccountUpdateOutcome {
    Updated { settings_revision: i64 },
    Conflict { expected: i64, current: i64 },
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountWorkspace {
    pub workspace_id: String,
    pub name: Option<String>,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountWorkspacePage {
    pub items: Vec<AccountWorkspace>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AccountWorkspaceListOutcome {
    Available { page: AccountWorkspacePage },
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}
