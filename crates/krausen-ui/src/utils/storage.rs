// SPDX-License-Identifier: AGPL-3.0-or-later
//! Browser local storage handle shared by the finite command-recovery records.

/// `label` names the record family in the error ("Evidence recovery", ...).
#[cfg(target_arch = "wasm32")]
pub fn local_storage(label: &str) -> Result<web_sys::Storage, String> {
    web_sys::window()
        .ok_or("Browser unavailable")?
        .local_storage()
        .map_err(|_| format!("{label} storage unavailable"))?
        .ok_or_else(|| format!("{label} storage unavailable"))
}
