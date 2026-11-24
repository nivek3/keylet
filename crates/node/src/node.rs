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
    read_key_pair(move || wallet_client::key_pair(&input, || false)).await
}

// Release builds never expose a path override, including to tests or environment variables.
#[cfg(debug_assertions)]
#[napi]
pub async fn get_key_pair_for_test(input: Value, executable: String) -> Result<JsKeyPair> {
    read_key_pair(move || wallet_client::key_pair_for_test(&input, executable.into(), || false))
        .await
}

async fn read_key_pair(
    request: impl FnOnce() -> std::result::Result<KeyPair, &'static str> + Send + 'static,
) -> Result<JsKeyPair> {
    let pair = blocking(request).await?;
    let (public_key, private_key) = pair.into_hex();
    Ok(JsKeyPair {
        public_key,
        private_key,
    })
}

#[napi(object)]
pub struct JsSignature {
    pub name: String,
    pub curve: String,
    pub public_key: String,
    pub signature: String,
    pub recovery_id: Option<u32>,
    pub address: Option<String>,
}

#[napi(object)]
pub struct JsSignerPublicKey {
    pub name: String,
    pub curve: String,
    pub public_key: String,
    pub address: Option<String>,
}

#[napi]
pub async fn sign_with_signer(input: Value) -> Result<JsSignature> {
    blocking(move || wallet_client::signer::sign(&input, || false))
        .await
        .map(signature)
}

#[napi]
pub async fn get_signer_public_key(input: Value) -> Result<JsSignerPublicKey> {
    blocking(move || wallet_client::signer::public_key(&input, || false))
        .await
        .map(signer_public_key)
}

#[cfg(debug_assertions)]
#[napi]
pub async fn sign_with_signer_for_test(input: Value, executable: String) -> Result<JsSignature> {
    blocking(move || wallet_client::signer::sign_for_test(&input, executable.into(), || false))
        .await
        .map(signature)
}

#[cfg(debug_assertions)]
#[napi]
pub async fn get_signer_public_key_for_test(
    input: Value,
    executable: String,
) -> Result<JsSignerPublicKey> {
    blocking(move || {
        wallet_client::signer::public_key_for_test(&input, executable.into(), || false)
    })
    .await
    .map(signer_public_key)
}

async fn blocking<T: Send + 'static>(
    request: impl FnOnce() -> std::result::Result<T, &'static str> + Send + 'static,
) -> Result<T> {
    // Authentication must not occupy Node's shared libuv worker pool.
    tokio::task::spawn_blocking(request)
        .await
        .map_err(|_| Error::from_reason("BRIDGE_ERROR"))?
        .map_err(Error::from_reason)
}

fn signature(value: wallet_client::signer::Signature) -> JsSignature {
    JsSignature {
        name: value.name,
        curve: value.curve.name().to_owned(),
        public_key: value.public_key,
        signature: value.signature,
        recovery_id: value.recovery_id.map(u32::from),
        address: value.address,
    }
}

fn signer_public_key(value: wallet_client::signer::SignerPublicKey) -> JsSignerPublicKey {
    JsSignerPublicKey {
        name: value.name,
        curve: value.curve.name().to_owned(),
        public_key: value.public_key,
        address: value.address,
    }
}
