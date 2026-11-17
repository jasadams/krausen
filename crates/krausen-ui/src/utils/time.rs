// SPDX-License-Identifier: AGPL-3.0-or-later

//! Shared time helpers for the Leptos frontend.

/// Format an RFC 3339 timestamp in the browser's locale and timezone.
///
/// The server keeps the original timestamp for a stable first render. The
/// client formats it in Rust/WASM instead of emitting a data-dependent inline
/// script, which keeps invitation pages compatible with nonce-only script CSP.
pub fn format_local_date_time(timestamp: &str) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        use wasm_bindgen::JsValue;

        let date = js_sys::Date::new(&JsValue::from_str(timestamp));
        if date.get_time().is_nan() {
            return timestamp.to_string();
        }

        let locales = js_sys::Array::new();
        let date_options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&date_options, &"year".into(), &"numeric".into());
        let _ = js_sys::Reflect::set(&date_options, &"month".into(), &"numeric".into());
        let _ = js_sys::Reflect::set(&date_options, &"day".into(), &"numeric".into());
        let time_options = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&time_options, &"hour".into(), &"numeric".into());
        let _ = js_sys::Reflect::set(&time_options, &"minute".into(), &"2-digit".into());
        let _ = js_sys::Reflect::set(&time_options, &"second".into(), &"2-digit".into());

        let format = |options: &js_sys::Object| {
            js_sys::Intl::DateTimeFormat::new(&locales, options)
                .format()
                .call1(&JsValue::NULL, &date)
                .ok()
                .and_then(|value| value.as_string())
        };
        match (format(&date_options), format(&time_options)) {
            (Some(date), Some(time)) => format!("{date} at {time}"),
            _ => timestamp.to_string(),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        timestamp.to_string()
    }
}
