// SPDX-License-Identifier: AGPL-3.0-or-later

//! krausen-types — Shared types for the Krausen backend.
//!
//! This crate holds cross-crate types used by krausen-core, krausen-auth,
//! krausen-ui, and the server binary.

pub mod brewery;
pub mod operations;
pub mod sync;
pub mod workspace;
pub mod invitation;
pub mod security_status;
pub mod account_read;
pub mod team_read;
pub mod session_read;
pub mod invitation_read;
pub mod team_invitation_read;

pub mod validation;

pub mod formulation;

pub mod recipe_editor;
pub mod request_context;

pub mod equipment;
pub mod interchange;
pub mod catalogue;

pub mod process_summary;
pub mod recipe_library;
pub mod scaling;
