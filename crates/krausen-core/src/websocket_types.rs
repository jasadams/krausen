// SPDX-License-Identifier: AGPL-3.0-or-later

//! WebSocket message types for real-time communication.

use serde::{Deserialize, Serialize};

/// Type of WebSocket message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    Heartbeat,
    Error,
    WorkspaceInvitation,
    WorkspaceRemoved,
    OwnershipTransferOffered,
    OwnershipTransferCompleted,
    OwnershipTransferDeclined,
    MemberRoleChanged,
    MemberJoined,
    SyncAction,
}

/// A WebSocket message sent to clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    #[serde(rename = "type")]
    pub msg_type: MessageType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl WebSocketMessage {
    pub fn new(msg_type: MessageType) -> Self {
        Self {
            msg_type,
            data: None,
        }
    }

    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }
}
