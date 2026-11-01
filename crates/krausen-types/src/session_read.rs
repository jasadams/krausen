// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub token_id: String,
    pub created_at: String,
    pub last_used: Option<String>,
    pub expires_at: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub country_code: Option<String>,
    pub oauth_client_name: Option<String>,
    pub is_current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPage {
    pub items: Vec<Session>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SessionListOutcome {
    Available { page: SessionPage },
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}
