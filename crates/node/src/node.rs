use super::{discovery::app_executable, hex, unlock, validate, BridgeResult, Selection};
use napi::{Error, Result};
use napi_derive::napi;
use serde_json::Value;
use std::path::PathBuf;

#[napi(object)]
pub struct JsKeyPair {
    pub public_key: String,
    pub private_key: String,
}

#[napi]
pub async fn get_key_pair(input: Value) -> Result<JsKeyPair> {
    read_key_pair(input, app_executable).await
}

// Release builds never expose a path override, including to tests or environment variables.
#[cfg(debug_assertions)]
#[napi]
pub async fn get_key_pair_for_test(input: Value, executable: String) -> Result<JsKeyPair> {
    read_key_pair(input, move || Ok(PathBuf::from(executable))).await
}

async fn read_key_pair(
    input: Value,
    discover: impl FnOnce() -> BridgeResult<PathBuf> + Send + 'static,
) -> Result<JsKeyPair> {
    // Authentication must not occupy Node's shared libuv worker pool.
    let output = tokio::task::spawn_blocking(move || {
        let (name, curve, path, authentication_ttl) = validate(&input)?;
        let helper = discover()?;
        unlock(Selection {
            helper,
            name,
            curve,
            path,
            authentication_ttl,
        })
    })
    .await
    .map_err(|_| Error::from_reason("BRIDGE_ERROR"))?
    .map_err(Error::from_reason)?;
    Ok(JsKeyPair {
        public_key: output.public,
        private_key: hex(&*output.private),
    })
}
