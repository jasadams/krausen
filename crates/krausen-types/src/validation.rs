// SPDX-License-Identifier: AGPL-3.0-or-later
//! Transport-neutral draft validation results.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Readiness {
    Ready,
    Incomplete,
    Invalid,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Target {
    Water,
    OriginalGravity,
    FinalGravity,
    Alcohol,
    Colour,
    Bitterness,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: String,
    pub path: String,
    pub severity: Readiness,
    pub affected_targets: Vec<Target>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Validation {
    pub state: Readiness,
    pub diagnostics: Vec<Diagnostic>,
}
impl Validation {
    pub fn saveable(&self) -> bool {
        self.state != Readiness::Invalid
    }
    pub fn startable(&self) -> bool {
        self.state == Readiness::Ready
    }
    pub fn target_available(&self, target: Target) -> bool {
        !self
            .diagnostics
            .iter()
            .any(|d| d.affected_targets.contains(&target))
    }
}
