// SPDX-License-Identifier: AGPL-3.0-or-later

//! The field plumbing shared by every BeerXML record.
//!
//! BeerXML 1.0 is around 150 scalar fields across ten records. Writing the
//! read-and-write code by hand for each would be a great deal of noise with
//! nothing interesting in it, so [`record!`] generates it from a declaration
//! of the fields and their tag names.

use crate::raw::RawElement;

/// A scalar field: the text exactly as it appeared, plus a parsed value where
/// one could be recovered.
///
/// The original text is always kept and is always what gets written back.
/// Real exports put display strings in numeric fields — beerxml.com's own
/// examples contain `<IBU>32.4 IBU</IBU>` — so parsing has to be forgiving
/// while export stays faithful. Keeping both means a consumer gets `32.4`
/// and the file still says `32.4 IBU` on the way out.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Scalar<T> {
    raw: String,
    parsed: Option<T>,
}

impl<T: Field> Scalar<T> {
    /// From text read out of a document.
    pub fn from_text(text: String) -> Self {
        let parsed = T::parse(&text);
        Self { raw: text, parsed }
    }

    /// From a value Krausen is writing itself.
    pub fn from_value(value: T) -> Self {
        Self {
            raw: value.write(),
            parsed: Some(value),
        }
    }
}

impl<T> Scalar<T> {
    /// The parsed value, if the text yielded one.
    pub const fn value(&self) -> Option<&T> {
        self.parsed.as_ref()
    }

    /// The text exactly as it appeared in the document.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// Whether the text carried more than the bare value — a unit suffix, say.
    pub fn is_exactly_parsed(&self) -> bool {
        self.parsed.is_some()
    }
}

/// How a field type reads from and writes to XML text.
pub trait Field: Sized {
    fn parse(text: &str) -> Option<Self>;
    fn write(&self) -> String;
}

impl Field for String {
    fn parse(text: &str) -> Option<Self> {
        Some(text.to_string())
    }
    fn write(&self) -> String {
        self.clone()
    }
}

impl Field for f64 {
    /// Takes the leading numeric portion, so `"32.4 IBU"` reads as `32.4`.
    /// The full text is preserved separately by [`Scalar`], so nothing is
    /// lost by being lenient here.
    fn parse(text: &str) -> Option<Self> {
        leading_number(text)?.parse().ok()
    }
    fn write(&self) -> String {
        // Keep a decimal point so the value still reads as a number to a
        // consumer that distinguishes them from integers.
        if self.fract() == 0.0 && self.is_finite() {
            format!("{self:.1}")
        } else {
            format!("{self}")
        }
    }
}

impl Field for u32 {
    fn parse(text: &str) -> Option<Self> {
        leading_number(text)?.parse().ok()
    }
    fn write(&self) -> String {
        self.to_string()
    }
}

impl Field for bool {
    /// BeerXML writes booleans as `TRUE`/`FALSE`, but exporters vary.
    fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_uppercase().as_str() {
            "TRUE" | "1" | "YES" => Some(true),
            "FALSE" | "0" | "NO" => Some(false),
            _ => None,
        }
    }
    fn write(&self) -> String {
        if *self { "TRUE" } else { "FALSE" }.to_string()
    }
}

/// The leading numeric portion of a string, if it starts with one.
fn leading_number(text: &str) -> Option<&str> {
    let text = text.trim();
    let mut end = 0;
    for (i, ch) in text.char_indices() {
        let acceptable = ch.is_ascii_digit()
            || (i == 0 && (ch == '-' || ch == '+'))
            || (ch == '.' && !text[..i].contains('.'));
        if acceptable {
            end = i + ch.len_utf8();
        } else {
            break;
        }
    }
    let head = &text[..end];
    if head.is_empty() || head == "-" || head == "+" || head == "." {
        None
    } else {
        Some(head)
    }
}

/// Shared behaviour for a generated record.
pub trait Record: Default {
    /// The element name, e.g. `"HOP"`.
    const TAG: &'static str;

    /// Take a scalar child element. Returns `false` if the tag is not one of
    /// this record's fields, in which case the caller preserves it as raw.
    fn set_scalar(&mut self, tag: &str, text: String) -> bool;

    /// Every scalar field that is set, in declaration order, for writing.
    fn scalars(&self) -> Vec<(&'static str, String)>;

    /// Elements that were not recognised, kept for round-tripping.
    fn extensions(&self) -> &[RawElement];

    /// Record an unrecognised element.
    fn push_extension(&mut self, element: RawElement);
}

/// Declare a BeerXML record: its element name, and each field with the tag it
/// maps to. Generates the struct and its [`Record`] implementation.
#[macro_export]
macro_rules! record {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident = $tag:literal {
            $(
                $(#[$fmeta:meta])*
                $field:ident : $ty:ty = $ftag:literal
            ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
        #[serde(default)]
        $vis struct $name {
            $(
                $(#[$fmeta])*
                pub $field: Option<$crate::record::Scalar<$ty>>,
            )*
            /// Elements not modelled by this crate, preserved verbatim.
            pub extensions: Vec<$crate::raw::RawElement>,
        }

        impl $name {
            $(
                /// The parsed value of this field, if present and parseable.
                pub fn $field(&self) -> Option<&$ty> {
                    self.$field.as_ref().and_then($crate::record::Scalar::value)
                }
            )*
        }

        impl $crate::record::Record for $name {
            const TAG: &'static str = $tag;

            fn set_scalar(&mut self, tag: &str, text: String) -> bool {
                match tag {
                    $(
                        $ftag => {
                            self.$field = Some($crate::record::Scalar::from_text(text));
                            true
                        }
                    )*
                    _ => false,
                }
            }

            fn scalars(&self) -> Vec<(&'static str, String)> {
                let mut out = Vec::new();
                $(
                    if let Some(v) = &self.$field {
                        out.push(($ftag, v.raw().to_string()));
                    }
                )*
                out
            }

            fn extensions(&self) -> &[$crate::raw::RawElement] {
                &self.extensions
            }

            fn push_extension(&mut self, element: $crate::raw::RawElement) {
                self.extensions.push(element);
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    record! {
        pub struct Sample = "SAMPLE" {
            name: String = "NAME",
            amount: f64 = "AMOUNT",
            count: u32 = "COUNT",
            flag: bool = "FLAG",
        }
    }

    #[test]
    fn known_tags_are_parsed_into_typed_fields() {
        let mut s = Sample::default();
        assert!(s.set_scalar("NAME", "Goldings".into()));
        assert!(s.set_scalar("AMOUNT", "0.02835".into()));
        assert!(s.set_scalar("COUNT", "3".into()));
        assert!(s.set_scalar("FLAG", "TRUE".into()));

        assert_eq!(s.name().map(String::as_str), Some("Goldings"));
        assert_eq!(s.amount().copied(), Some(0.02835));
        assert_eq!(s.count().copied(), Some(3));
        assert_eq!(s.flag().copied(), Some(true));
    }

    #[test]
    fn an_unknown_tag_is_refused_so_the_caller_can_preserve_it() {
        let mut s = Sample::default();
        assert!(!s.set_scalar("GRAINFATHER_THING", "x".into()));
    }

    #[test]
    fn an_unparseable_number_is_kept_rather_than_dropped_or_defaulted() {
        // Real exports contain empty and malformed numeric elements. Losing
        // them, or turning them into a confident zero, are both worse than
        // keeping what was actually there.
        let mut s = Sample::default();
        s.set_scalar("AMOUNT", "not a number".into());
        assert_eq!(s.amount(), None, "it did not parse");
        assert_eq!(
            s.scalars(),
            vec![("AMOUNT", "not a number".to_string())],
            "and it still writes back unchanged"
        );
    }

    #[test]
    fn a_number_with_a_unit_suffix_parses_and_still_writes_verbatim() {
        // beerxml.com's own examples contain <IBU>32.4 IBU</IBU>.
        let mut s = Sample::default();
        s.set_scalar("AMOUNT", "32.4 IBU".into());
        assert_eq!(s.amount().copied(), Some(32.4), "the value is usable");
        assert_eq!(
            s.scalars(),
            vec![("AMOUNT", "32.4 IBU".to_string())],
            "and the file still says what it said"
        );
    }

    #[test]
    fn leading_number_handles_the_shapes_that_turn_up() {
        assert_eq!(leading_number("32.4 IBU"), Some("32.4"));
        assert_eq!(leading_number("  1.050 SG "), Some("1.050"));
        assert_eq!(leading_number("-2.5 C"), Some("-2.5"));
        assert_eq!(leading_number("18.92716800"), Some("18.92716800"));
        // A second decimal point ends the number rather than corrupting it.
        assert_eq!(leading_number("1.2.3"), Some("1.2"));
        assert_eq!(leading_number("not a number"), None);
        assert_eq!(leading_number(""), None);
        assert_eq!(leading_number("-"), None);
        assert_eq!(leading_number("."), None);
    }

    #[test]
    fn a_value_krausen_writes_itself_round_trips_through_raw() {
        let s: Scalar<f64> = Scalar::from_value(4.5);
        assert_eq!(s.value().copied(), Some(4.5));
        assert_eq!(s.raw(), "4.5");
    }

    #[test]
    fn an_empty_element_survives_the_round_trip() {
        let mut s = Sample::default();
        s.set_scalar("AMOUNT", String::new());
        assert_eq!(s.scalars(), vec![("AMOUNT", String::new())]);
    }

    #[test]
    fn booleans_accept_what_exporters_actually_write() {
        for truthy in ["TRUE", "true", "1", "yes"] {
            assert_eq!(bool::parse(truthy), Some(true), "{truthy}");
        }
        for falsy in ["FALSE", "false", "0", "no"] {
            assert_eq!(bool::parse(falsy), Some(false), "{falsy}");
        }
        assert_eq!(bool::parse("maybe"), None);
    }

    #[test]
    fn whole_numbers_still_write_as_decimals() {
        // A consumer reading AMOUNT expects a number, not an integer.
        assert_eq!(Field::write(&5.0_f64), "5.0");
        assert_eq!(Field::write(&0.02835_f64), "0.02835");
    }

    #[test]
    fn only_fields_that_were_set_are_written() {
        let mut s = Sample::default();
        s.set_scalar("NAME", "x".into());
        assert_eq!(s.scalars(), vec![("NAME", "x".to_string())]);
    }

    #[test]
    fn fields_are_written_in_declaration_order() {
        let mut s = Sample::default();
        s.set_scalar("FLAG", "TRUE".into());
        s.set_scalar("NAME", "x".into());
        s.set_scalar("COUNT", "1".into());
        let tags: Vec<&str> = s.scalars().into_iter().map(|(t, _)| t).collect();
        assert_eq!(tags, vec!["NAME", "COUNT", "FLAG"]);
    }
}
