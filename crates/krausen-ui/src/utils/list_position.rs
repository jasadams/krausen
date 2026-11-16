// SPDX-License-Identifier: AGPL-3.0-or-later
//! Remembers how far an authorized library list was paged and scrolled so a
//! return from a recipe restores that position (IA-007). Only the library
//! location (search/filter), a page count and a scroll offset are kept, in the
//! tab's session storage, keyed by account and workspace; no recipe content.
//! A saved position is consumed by the one return that uses it.

/// Upper bound on pages replayed during a restore (50 rows per page).
pub const MAX_RESTORE_PAGES: u32 = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListPosition {
    pub pages: u32,
    pub scroll_top: i32,
    /// The library location (`/recipes?...`) the position belongs to.
    pub location: String,
}

pub fn encode(position: &ListPosition) -> String {
    format!("{}:{}:{}", position.pages, position.scroll_top, position.location)
}

pub fn decode(raw: &str) -> Option<ListPosition> {
    let mut parts = raw.splitn(3, ':');
    let pages = parts.next()?.parse::<u32>().ok()?;
    let scroll_top = parts.next()?.parse::<i32>().ok()?;
    let location = parts.next()?.to_owned();
    (pages >= 1
        && pages <= MAX_RESTORE_PAGES
        && scroll_top >= 0
        && (location == "/recipes" || location.starts_with("/recipes?")))
    .then_some(ListPosition { pages, scroll_top, location })
}

pub fn storage_key(account_id: &str, workspace_id: &str) -> String {
    format!("krausen-library-position:{account_id}:{workspace_id}")
}

#[cfg(target_arch = "wasm32")]
fn session() -> Option<web_sys::Storage> {
    web_sys::window()?.session_storage().ok().flatten()
}

#[cfg(target_arch = "wasm32")]
pub fn save(key: &str, position: &ListPosition) {
    if let Some(storage) = session() {
        if storage.set_item(key, &encode(position)).is_err() {
            tracing::warn!("Unable to remember the library position");
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub fn take(key: &str) -> Option<ListPosition> {
    let storage = session()?;
    let raw = storage.get_item(key).ok().flatten()?;
    if storage.remove_item(key).is_err() {
        tracing::warn!("Unable to clear the remembered library position");
    }
    decode(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_rejects_malformed_unbounded_or_foreign_values() {
        let position = ListPosition { pages: 3, scroll_top: 640, location: "/recipes?q=a:b&show=archived".into() };
        assert_eq!(decode(&encode(&position)), Some(position));
        for bad in ["", "3", "0:5:/recipes", "21:5:/recipes", "x:1:/recipes", "2:-4:/recipes", "2:y:/recipes", "2:3:", "2:3:https://evil.example", "2:3://evil.example"] {
            assert_eq!(decode(bad), None, "{bad}");
        }
    }

    #[test]
    fn key_is_scoped_by_account_and_workspace() {
        assert_ne!(storage_key("a", "w"), storage_key("b", "w"));
        assert_ne!(storage_key("a", "w"), storage_key("a", "x"));
    }
}
