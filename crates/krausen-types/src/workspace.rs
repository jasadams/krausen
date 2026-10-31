// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkspaceRenameOutcome {
    Saved { revision: i64 },
    Conflict { expected: i64, actual: i64 },
    RequestIdentityConflict,
    ResultExpired { resource_id: Option<String> },
    Forbidden,
    Invalid,
    Retryable,
    Unavailable,
}
