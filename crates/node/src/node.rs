use napi::{Error, Result};
use napi_derive::napi;
use serde_json::Value;
use wallet_client::KeyPair;

#[napi(object)]
pub struct JsKeyPair {
    pub public_key: String,
    pub private_key: String,
}

#[napi]
pub async fn get_key_pair(input: Value) -> Result<JsKeyPair> {
    read_key_pair(move || wallet_client::key_pair(&input)).await
}

// Release builds never expose a path override, including to tests or environment variables.
#[cfg(debug_assertions)]
#[napi]
pub async fn get_key_pair_for_test(input: Value, executable: String) -> Result<JsKeyPair> {
    read_key_pair(move || wallet_client::key_pair_for_test(&input, executable.into())).await
}

async fn read_key_pair(
    request: impl FnOnce() -> std::result::Result<KeyPair, &'static str> + Send + 'static,
) -> Result<JsKeyPair> {
    // Authentication must not occupy Node's shared libuv worker pool.
    let pair = tokio::task::spawn_blocking(request)
        .await
        .map_err(|_| Error::from_reason("BRIDGE_ERROR"))?
        .map_err(Error::from_reason)?;
    let (public_key, private_key) = pair.into_hex();
    Ok(JsKeyPair {
        public_key,
        private_key,
    })
}
