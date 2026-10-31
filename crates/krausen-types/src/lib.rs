// SPDX-License-Identifier: AGPL-3.0-or-later

//! krausen-types — Shared types for the Krausen backend.
//!
//! This crate holds cross-crate types used by krausen-core, krausen-auth,
//! krausen-ui, and the server binary.

pub mod brewery;
pub mod operations;
pub mod sync;
pub mod workspace;

pub mod validation;

pub mod formulation;

pub mod request_context;
pub mod recipe_editor;

pub mod equipment;
pub mod interchange;
