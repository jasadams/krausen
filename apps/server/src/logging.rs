// SPDX-License-Identifier: AGPL-3.0-or-later
//! Logging boundary for third-party protocol internals.
//!
//! The MCP SDK can log complete caller-controlled requests, notifications and
//! responses even at info level. Application-owned diagnostics must instead
//! record fixed operation identifiers and sanitized outcomes.

/// Never send SDK payload events or spans to the application log sink.
/// This filter is composed independently of operator-provided `RUST_LOG`.
pub fn allow_metadata(metadata: &tracing::Metadata<'_>) -> bool {
    let target = metadata.target();
    target != "rmcp" && !target.starts_with("rmcp::")
}
