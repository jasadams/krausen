// SPDX-License-Identifier: AGPL-3.0-or-later

//! The escape hatch that makes round-tripping lossless.
//!
//! BeerXML in the wild is not only BeerXML. Exporters add their own elements,
//! and a future version of the standard will add more. Anything this crate
//! does not recognise is kept verbatim as a [`RawElement`] and written back
//! out unchanged, so importing a recipe and exporting it again never quietly
//! costs the brewer data they had before Krausen touched it.

use serde::{Deserialize, Serialize};

/// An element this crate does not model, preserved as it was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawElement {
    pub name: String,
    /// Text content, when the element has any.
    pub text: Option<String>,
    /// Child elements, for extensions that are themselves structured.
    pub children: Vec<RawElement>,
}

impl RawElement {
    /// A leaf element with text content.
    pub fn leaf(name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            text: Some(text.into()),
            children: Vec::new(),
        }
    }

    /// Whether this element has no text and no children.
    pub fn is_empty(&self) -> bool {
        self.text.as_ref().is_none_or(|t| t.is_empty()) && self.children.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leaf_carries_its_text() {
        let e = RawElement::leaf("GRAINFATHER_ID", "12345");
        assert_eq!(e.name, "GRAINFATHER_ID");
        assert_eq!(e.text.as_deref(), Some("12345"));
        assert!(e.children.is_empty());
        assert!(!e.is_empty());
    }

    #[test]
    fn emptiness_covers_both_missing_and_blank_text() {
        assert!(RawElement {
            name: "X".into(),
            text: None,
            children: vec![]
        }
        .is_empty());
        assert!(RawElement::leaf("X", "").is_empty());
        assert!(!RawElement::leaf("X", " ").is_empty());
    }
}
