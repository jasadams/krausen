// SPDX-License-Identifier: AGPL-3.0-or-later

//! RFC 8785 JSON bytes and SHA-256 for already validated JSON values.

use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    #[error("integer exceeds the interoperable exact range")]
    IntegerOutOfRange,
    #[error("non-finite number is not JSON")]
    NonFinite,
    #[error(transparent)]
    Serialize(#[from] serde_value::SerializerError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Capture numbers before JSON conversion, rejecting NaN and infinities rather
/// than allowing a serializer to turn them into null.
pub fn from_serializable<T: serde::Serialize>(input: &T) -> Result<Value, CanonicalError> {
    fn check(value: &serde_value::Value) -> Result<(), CanonicalError> {
        use serde_value::Value as V;
        match value {
            V::F32(n) if !n.is_finite() => return Err(CanonicalError::NonFinite),
            V::F64(n) if !n.is_finite() => return Err(CanonicalError::NonFinite),
            V::Option(Some(v)) | V::Newtype(v) => check(v)?,
            V::Seq(values) => {
                for v in values {
                    check(v)?;
                }
            }
            V::Map(values) => {
                for (k, v) in values {
                    check(k)?;
                    check(v)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let value = serde_value::to_value(input)?;
    check(&value)?;
    let json = serde_json::to_value(value)?;
    // Perform the integer-range checks before returning a usable value.
    bytes(&json)?;
    Ok(json)
}

/// JSON values cannot represent non-finite numbers. For Rust values use
/// [`from_serializable`] before this function to detect them before conversion.
pub fn bytes(value: &Value) -> Result<Vec<u8>, CanonicalError> {
    fn check(value: &Value) -> Result<(), CanonicalError> {
        const MAX: u64 = 9_007_199_254_740_991;
        match value {
            Value::Number(n) => {
                if n.as_u64().is_some_and(|x| x > MAX)
                    || n.as_i64().is_some_and(|x| x.unsigned_abs() > MAX)
                {
                    return Err(CanonicalError::IntegerOutOfRange);
                }
            }
            Value::Array(items) => {
                for item in items {
                    check(item)?;
                }
            }
            Value::Object(items) => {
                for item in items.values() {
                    check(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    check(value)?;
    Ok(serde_json_canonicalizer::to_vec(value)?)
}

pub fn sha256(value: &Value) -> Result<String, CanonicalError> {
    Ok(format!("{:x}", Sha256::digest(bytes(value)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_numbers_and_keys_and_rejects_unsafe_integers() {
        let value: Value = serde_json::from_str(r#"{"numbers":[333333333.33333329,1E30,4.50,2e-3,0.000000000000000000000000001],"a":-0.0}"#).unwrap();
        assert_eq!(
            String::from_utf8(bytes(&value).unwrap()).unwrap(),
            r#"{"a":0,"numbers":[333333333.3333333,1e+30,4.5,0.002,1e-27]}"#
        );
        assert!(matches!(
            bytes(&serde_json::json!({"value": 9007199254740992u64})),
            Err(CanonicalError::IntegerOutOfRange)
        ));
        assert_eq!(
            sha256(&serde_json::json!({})).unwrap(),
            "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
        let unicode = serde_json::json!({"\u{e000}": 1, "\u{10000}": 2});
        assert_eq!(
            String::from_utf8(bytes(&unicode).unwrap()).unwrap(),
            "{\"𐀀\":2,\"\":1}"
        );
        assert!(matches!(
            from_serializable(&vec![f64::NAN]),
            Err(CanonicalError::NonFinite)
        ));
        assert!(matches!(
            from_serializable(&f64::INFINITY),
            Err(CanonicalError::NonFinite)
        ));
        let nested = serde_json::json!({"z": [3, {"b": "line\n\"\\", "a": 1}, 2]});
        assert_eq!(
            String::from_utf8(bytes(&nested).unwrap()).unwrap(),
            r#"{"z":[3,{"a":1,"b":"line\n\"\\"},2]}"#
        );
        assert_eq!(from_serializable(&nested).unwrap(), nested);
    }
}
