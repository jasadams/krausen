// SPDX-License-Identifier: AGPL-3.0-or-later

//! Errors from reading BeerXML.

use thiserror::Error;

/// Why a document could not be read.
///
/// Only genuine XML failures appear here. A document whose *contents* are
/// surprising parses successfully — unrecognised elements are preserved, not
/// rejected — so this is a short list by design.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("malformed XML at byte {position}: {message}")]
    Xml { position: u64, message: String },

    #[error("the document has no root element")]
    NoRootElement,
}
