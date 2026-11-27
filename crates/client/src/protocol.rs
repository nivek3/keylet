use super::{BridgeResult, ClientError};
use serde::{
    Deserialize, Deserializer,
    de::{DeserializeOwned, IntoDeserializer},
};
use serde_json::{Map, Value};
use wallet_crypto::Curve;

pub(super) fn request<T: DeserializeOwned>(input: &Value, error: ClientError) -> BridgeResult<T> {
    if !input.is_object() {
        return Err(error);
    }
    T::deserialize(input).map_err(|_| error)
}

pub(super) fn reply<T: DeserializeOwned>(line: &[u8], err: &[u8]) -> BridgeResult<T> {
    let invalid = ClientError::InvalidHelperResponse;
    if !err.iter().all(u8::is_ascii_whitespace) {
        return Err(invalid);
    }
    // Require a JSON object. Struct deserializers also accept JSON arrays.
    let object: Map<String, Value> = serde_json::from_slice(line).map_err(|_| invalid)?;
    serde_json::from_value(Value::Object(object)).map_err(|_| invalid)
}

// With serde(default), a missing field is None. Present values must decode as T.
pub(super) fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

pub(super) fn curve<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Curve, D::Error> {
    // The wire format only accepts a string, including for unit enum variants.
    Curve::deserialize(String::deserialize(deserializer)?.into_deserializer())
}

pub(super) fn auth_seconds(value: f64) -> Option<u64> {
    ((0.0..=9_007_199_254_740_991.0).contains(&value) && value.fract() == 0.0)
        .then_some(value as u64)
}
