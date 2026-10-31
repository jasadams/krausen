// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cryptographically random request IDs, including on HTTP LAN origins where
//! randomUUID may be unavailable but getRandomValues is still supported.

use wasm_bindgen::JsCast;

fn uuid_from_random(mut bytes: [u8; 16]) -> String {
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

fn select_id(
    primary: impl FnOnce() -> Option<String>,
    random: impl FnOnce() -> Option<[u8; 16]>,
) -> Option<String> {
    primary().or_else(|| random().map(uuid_from_random))
}

pub fn new_request_id() -> Option<String> {
    let crypto = web_sys::window()?.crypto().ok()?;
    select_id(
        || {
            let value = js_sys::Reflect::get(crypto.as_ref(), &"randomUUID".into()).ok()?;
            let function = value.dyn_into::<js_sys::Function>().ok()?;
            function.call0(crypto.as_ref()).ok()?.as_string()
        },
        || {
            let mut bytes = [0u8; 16];
            crypto.get_random_values_with_u8_array(&mut bytes).ok()?;
            Some(bytes)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_uses_random_bytes_with_uuid_v4_bits() {
        assert_eq!(
            select_id(|| None, || Some([0xff; 16])).as_deref(),
            Some("ffffffff-ffff-4fff-bfff-ffffffffffff")
        );
        assert!(select_id(|| None, || None).is_none());
    }

    #[test]
    fn available_random_uuid_is_used_first() {
        assert_eq!(
            select_id(
                || Some("browser-id".into()),
                || panic!("fallback must not run")
            )
            .as_deref(),
            Some("browser-id")
        );
    }
}
