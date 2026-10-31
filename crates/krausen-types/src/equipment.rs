// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared equipment profile operation contracts.
use crate::{brewery::EquipmentSettingsV1, request_context::OperationIntent};
use serde::{Deserialize, Serialize};

pub use crate::brewery::{EquipmentProfile, EquipmentProvenance, EquipmentRevision};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SaveEquipment {
    pub intent: OperationIntent,
    pub profile_id: String,
    pub name: String,
    pub settings: EquipmentSettingsV1,
    pub provenance: EquipmentProvenance,
    pub expected_revision: i64,
    pub expected_metadata_revision: i64,
    pub request_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ArchiveEquipment {
    pub intent: OperationIntent,
    pub profile_id: String,
    pub archived: bool,
    pub expected_metadata_revision: i64,
    pub request_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EquipmentOutcome {
    Profile(EquipmentProfile),
    Profiles(Vec<EquipmentProfile>),
    Revision(EquipmentRevision),
    NotFound,
    Archived,
    Conflict { expected: i64, actual: i64 },
    RequestIdentityConflict,
    ResultExpired,
    ContextChanged,
    Forbidden,
    Invalid,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EquipmentQuery {
    pub intent: OperationIntent,
    pub profile_id: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EquipmentRevisionQuery {
    pub intent: OperationIntent,
    pub profile_id: String,
    pub revision: i64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct EquipmentListQuery {
    pub intent: OperationIntent,
    pub after: String,
    pub limit: i64,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EquipmentCommand {
    Save(SaveEquipment),
    Archive(ArchiveEquipment),
    Get(EquipmentQuery),
    List(EquipmentListQuery),
    Revision(EquipmentRevisionQuery),
}
