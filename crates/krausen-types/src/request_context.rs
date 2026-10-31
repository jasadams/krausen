// SPDX-License-Identifier: AGPL-3.0-or-later
//! Untrusted caller expectations for preventing stale-context operations.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct OperationIntent {
    pub account_id: String,
    pub workspace_id: String,
}

/// Encode a JSON request without silently changing non-finite numbers to null.
pub fn checked_json<T: serde::Serialize>(input: &T) -> Result<String, ()> {
    fn finite(v: &serde_value::Value) -> bool {
        use serde_value::Value as V;
        match v {
            V::F32(n) => n.is_finite(),
            V::F64(n) => n.is_finite(),
            V::Option(Some(v)) | V::Newtype(v) => finite(v),
            V::Seq(values) => values.iter().all(finite),
            V::Map(values) => values.iter().all(|(k, v)| finite(k) && finite(v)),
            _ => true,
        }
    }
    let v = serde_value::to_value(input).map_err(|_| ())?;
    if !finite(&v) {
        return Err(());
    }
    serde_json::to_string(input).map_err(|_| ())
}
