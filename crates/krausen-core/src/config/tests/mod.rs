// SPDX-License-Identifier: AGPL-3.0-or-later

mod smtp_settings;

/// `config.rs`'s own source, for the guard tests that pin which call sites
/// route through the shared predicate rather than re-spelling it.
const SRC: &str = include_str!("../../config.rs");
