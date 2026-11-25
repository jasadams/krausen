// SPDX-License-Identifier: AGPL-3.0-or-later
//! Additive local URL state and safe in-app return locations (IA-007, IA-008).
//!
//! Encoding, chosen and documented here (no pre-existing query API is assumed):
//!
//! * `/recipe-editor/:id?section=versions|brews[&revision=N]` — the recipe-local
//!   section and the exact immutable saved version being inspected. The bare
//!   path is the editable recipe. `revision` never loads into the draft.
//! * `/recipes?q=TEXT&show=archived[&versions=RECIPE_ID&revision=N]` — library
//!   search, archive filter and the inline saved-versions selection.
//! * `/settings/...?return=/in-app/path` — where Settings returns to.

/// Longest accepted return location; anything longer is treated as invalid.
const MAX_RETURN_LEN: usize = 2048;

/// Routes that must never be a return destination: ceremony, callback and
/// Settings-internal locations (returning to Settings would trap the user).
const NON_RETURN_PREFIXES: &[&str] = &[
    "/login",
    "/signup",
    "/welcome",
    "/auth/",
    "/account/",
    "/oauth-complete",
    "/settings",
    "/accept-ownership",
    "/accept-invite",
];

/// Percent-encodes everything outside the RFC 3986 unreserved set.
pub fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// Reverses [`encode_component`]; `+` is read as a space for hand-typed query
/// strings although `encode_component` itself only ever emits `%20`. Malformed escapes are rejected, not guessed.
pub fn decode_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = value.get(index + 1..index + 3)?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

/// Library filter carried by a `/recipes?...` location: search text and archive flag.
pub fn library_filter(location: &str) -> Option<(String, bool)> {
    let query = location.strip_prefix("/recipes")?.strip_prefix('?').unwrap_or("");
    let mut search = String::new();
    let mut archived = false;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "q" => search = decode_component(value)?,
            "show" => archived = value == "archived",
            _ => {}
        }
    }
    Some((search, archived))
}

/// `/recipes/:id/brews`, the recipe-local Brews page.
pub fn brews_href(recipe_id: &str) -> String {
    format!("/recipes/{}/brews", encode_component(recipe_id))
}

/// `/brews/:id` (Measurements) or `/brews/:id/analysis` (Compare).
pub fn brew_href(brew_id: &str, compare: bool) -> String {
    let base = format!("/brews/{}", encode_component(brew_id));
    if compare { format!("{base}/analysis") } else { base }
}

/// Accepts only a same-origin, in-app absolute path (with optional query/hash).
/// Rejects scheme/authority forms, protocol-relative and backslash tricks,
/// control characters, ceremony/Settings routes and oversized input. The
/// router-decoded query value is what is validated, so double encoding that
/// decodes into an external form is rejected here too.
///
/// General Settings return rule. Equipment's `safe_return` (pages/equipment.rs,
/// owned by its own ticket) is deliberately narrower: recipe-editor paths only.
pub fn safe_return(raw: &str) -> Option<String> {
    if raw.is_empty() || raw.len() > MAX_RETURN_LEN {
        return None;
    }
    if raw.chars().any(|c| c.is_control() || c == '\\' || c.is_whitespace()) {
        return None;
    }
    if !raw.starts_with('/') || raw.starts_with("//") {
        return None;
    }
    let path = raw.split(['?', '#']).next().unwrap_or(raw);
    if path.contains("//") || path.split('/').any(|part| part == "..") {
        return None;
    }
    if NON_RETURN_PREFIXES
        .iter()
        .any(|prefix| path == prefix.trim_end_matches('/') || path.starts_with(prefix))
    {
        return None;
    }
    // A percent-encoded form that decodes into a separator or scheme trick.
    let lowered = raw.to_ascii_lowercase();
    if lowered.contains("%2f%2f") || lowered.contains("%5c") || lowered.contains("%00") {
        return None;
    }
    Some(raw.to_owned())
}

/// Adds the validated return location to a Settings href; a missing or invalid
/// location leaves the href unchanged so Settings falls back to Recipes.
pub fn settings_href(path: &str, return_to: Option<&str>) -> String {
    match return_to.and_then(safe_return) {
        Some(location) => format!("{path}?return={}", encode_component(&location)),
        None => path.to_owned(),
    }
}

/// The sidebar's link to Settings from the current location.
pub fn current_location(pathname: &str, search: &str) -> String {
    if search.is_empty() {
        pathname.to_owned()
    } else if search.starts_with('?') {
        format!("{pathname}{search}")
    } else {
        format!("{pathname}?{search}")
    }
}

/// Recipe-local section addressed by the additive `section` parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecipeSection {
    Recipe,
    Versions,
    Brews,
}

impl RecipeSection {
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("versions") => Self::Versions,
            Some("brews") => Self::Brews,
            _ => Self::Recipe,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Recipe => "recipe",
            Self::Versions => "versions",
            Self::Brews => "brews",
        }
    }
}

/// A positive saved-version number; anything else is no selection.
pub fn parse_revision(value: Option<&str>) -> Option<i64> {
    value
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|revision| *revision > 0)
}

/// Editor location for a recipe section and optional exact saved version.
pub fn recipe_section_href(recipe_id: &str, section: RecipeSection, revision: Option<i64>) -> String {
    let base = format!("/recipe-editor/{}", encode_component(recipe_id));
    match (section, revision) {
        (RecipeSection::Recipe, _) => base,
        (section, Some(revision)) if section == RecipeSection::Versions => {
            format!("{base}?section=versions&revision={revision}")
        }
        (section, _) => format!("{base}?section={}", section.key()),
    }
}

/// Library location carrying search, archive filter and inline selection.
pub fn library_href(
    search: &str,
    archived: bool,
    versions: Option<(&str, Option<i64>)>,
) -> String {
    let mut parts = Vec::new();
    if !search.is_empty() {
        parts.push(format!("q={}", encode_component(search)));
    }
    if archived {
        parts.push("show=archived".to_owned());
    }
    if let Some((recipe_id, revision)) = versions {
        parts.push(format!("versions={}", encode_component(recipe_id)));
        if let Some(revision) = revision {
            parts.push(format!("revision={revision}"));
        }
    }
    if parts.is_empty() {
        "/recipes".to_owned()
    } else {
        format!("/recipes?{}", parts.join("&"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_in_app_locations() {
        for good in ["/recipes", "/recipes?q=a%20b&show=archived", "/brews/abc/analysis", "/equipment"] {
            assert_eq!(safe_return(good).as_deref(), Some(good), "{good}");
        }
        for bad in [
            "", "recipes", "//evil.example", "/\\evil.example", "https://evil.example/x",
            "javascript:alert(1)", "/recipes\n", "/a b", "/../etc", "/a//b", "/settings/profile",
            "/login", "/auth/google/callback", "/account/recover", "/%2f%2fevil", "/x%5cevil",
            "/oauth-complete", "/accept-ownership/1", "/accept-invite/inv-1",
        ] {
            assert_eq!(safe_return(bad), None, "{bad}");
        }
        assert_eq!(safe_return(&format!("/{}", "a".repeat(MAX_RETURN_LEN))), None);
    }

    #[test]
    fn settings_href_round_trips_valid_returns_and_drops_invalid() {
        let href = settings_href("/settings/team", Some("/recipes?q=a&show=archived"));
        assert_eq!(href, "/settings/team?return=%2Frecipes%3Fq%3Da%26show%3Darchived");
        assert_eq!(settings_href("/settings/team", Some("https://evil.example")), "/settings/team");
        assert_eq!(settings_href("/settings/team", None), "/settings/team");
    }

    #[test]
    fn section_and_revision_encoding_is_additive_and_strict() {
        assert_eq!(RecipeSection::parse(None), RecipeSection::Recipe);
        assert_eq!(RecipeSection::parse(Some("versions")), RecipeSection::Versions);
        assert_eq!(RecipeSection::parse(Some("history")), RecipeSection::Recipe);
        assert_eq!(parse_revision(Some("3")), Some(3));
        for bad in [None, Some("0"), Some("-1"), Some("x"), Some("1.5"), Some("")] {
            assert_eq!(parse_revision(bad), None);
        }
        assert_eq!(recipe_section_href("r1", RecipeSection::Recipe, Some(2)), "/recipe-editor/r1");
        assert_eq!(recipe_section_href("r1", RecipeSection::Versions, Some(2)), "/recipe-editor/r1?section=versions&revision=2");
        assert_eq!(recipe_section_href("r1", RecipeSection::Versions, None), "/recipe-editor/r1?section=versions");
        assert_eq!(recipe_section_href("r1", RecipeSection::Brews, Some(2)), "/recipe-editor/r1?section=brews");
    }

    #[test]
    fn decode_reverses_encode_and_rejects_malformed_escapes() {
        for value in ["Hel les", "a&b=c", "ü ß", "100%"] {
            assert_eq!(decode_component(&encode_component(value)).as_deref(), Some(value));
        }
        assert_eq!(decode_component("%zz"), None);
        assert_eq!(decode_component("%4"), None);
        assert_eq!(library_filter("/recipes?q=Hel%20les&show=archived"), Some(("Hel les".into(), true)));
        assert_eq!(library_filter("/recipes"), Some((String::new(), false)));
        assert_eq!(library_filter("/other"), None);
    }

    #[test]
    fn library_href_carries_search_filter_and_selection() {
        assert_eq!(library_href("", false, None), "/recipes");
        assert_eq!(library_href("Hel les", true, None), "/recipes?q=Hel%20les&show=archived");
        assert_eq!(library_href("x", false, Some(("r1", Some(4)))), "/recipes?q=x&versions=r1&revision=4");
        assert_eq!(current_location("/recipes", "?q=x"), "/recipes?q=x");
        assert_eq!(current_location("/recipes", ""), "/recipes");
    }

    #[test]
    fn brews_href_points_at_the_recipe_local_brews_page() {
        assert_eq!(brews_href("r1"), "/recipes/r1/brews");
        assert_eq!(brews_href("a/b"), "/recipes/a%2Fb/brews");
    }
}
