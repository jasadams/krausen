// SPDX-License-Identifier: AGPL-3.0-or-later
//! Untrusted caller expectations for preventing stale-context operations.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OperationIntent {
    pub account_id: String,
    pub workspace_id: String,
}
