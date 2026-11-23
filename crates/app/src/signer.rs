use crate::{
    emit,
    macos::{self, AuthContext, Vault},
    model::{self, Failure, Result},
};
use sha3::{Digest, Sha3_256};
use solana_ed25519::ed_sigs::SigningKey;
use std::io::Read;

const MAX_PAYLOAD_BYTES: usize = 1232;

fn address(key: &SigningKey) -> String {
    let public_key: [u8; 32] = key.verification_key().into();
    bs58::encode(public_key).into_string()
}

fn read_payload(input: impl Read) -> Result<Vec<u8>> {
    let mut payload = Vec::new();
    input
        .take((MAX_PAYLOAD_BYTES + 1) as u64)
        .read_to_end(&mut payload)
        .map_err(|_| Failure::Transport)?;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Failure::Input);
    }
    Ok(payload)
}

fn signature_response(name: &str, key: &SigningKey, payload: &[u8]) -> serde_json::Value {
    let signature = model::hex(&key.sign(payload).to_bytes());
    serde_json::json!({"name": name, "address": address(key), "signature": signature})
}

pub fn run(vault: &Vault, operation: &str, name: &str) -> Result<()> {
    model::validate_name(name)?;
    match operation {
        "create" => {
            let seed = macos::random_secret::<32>()?;
            let key = SigningKey::from_bytes(&seed);
            vault.insert_signer(name, &seed)?;
            emit(&serde_json::json!({"name": name, "address": address(&key)}))
        }
        "address" => {
            macos::initialize_accessory()?;
            let context = AuthContext::new(
                &format!("read the address of local signer \"{name}\""),
                false,
            );
            let seed = vault.read_signer(name, &context)?;
            let key = SigningKey::from_bytes(&seed);
            emit(&serde_json::json!({"name": name, "address": address(&key)}))
        }
        "sign" => {
            let payload = read_payload(std::io::stdin().lock())?;
            macos::initialize_accessory()?;
            let fingerprint = model::hex(&Sha3_256::digest(&payload));
            let reason = format!(
                "sign {} unverified bytes with local signer \"{name}\"\nSHA3-256: {fingerprint}",
                payload.len()
            );
            let context = AuthContext::new(&reason, false);
            let seed = vault.read_signer(name, &context)?;
            emit(&signature_response(
                name,
                &SigningKey::from_bytes(&seed),
                &payload,
            ))
        }
        "delete" => {
            macos::initialize_accessory()?;
            vault.delete_signer(name)?;
            emit(&serde_json::json!({"name": name, "deleted": true}))
        }
        _ => Err(Failure::Input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_input_is_bounded_and_preserves_exact_bytes() {
        assert_eq!(read_payload(&[][..]), Err(Failure::Input));
        assert_eq!(
            read_payload(&[0; MAX_PAYLOAD_BYTES + 1][..]),
            Err(Failure::Input)
        );
        let mut payload = [0; MAX_PAYLOAD_BYTES];
        payload[1] = b'\n';
        payload[2] = 255;
        assert_eq!(read_payload(&payload[..]).unwrap(), payload);
    }

    #[test]
    fn signature_response_verifies_against_the_reported_address() {
        let key = SigningKey::from_bytes(&[17; 32]);
        let payload = read_payload(&b"test\x00\n"[..]).unwrap();
        let response = signature_response("agent", &key, &payload);
        assert_eq!(response["name"], "agent");

        let address = response["address"].as_str().unwrap();
        let public_key: [u8; 32] = bs58::decode(address)
            .into_vec()
            .unwrap()
            .try_into()
            .unwrap();
        let public_key = solana_ed25519::ed_sigs::VerificationKey::try_from(public_key).unwrap();

        let hex = response["signature"].as_str().unwrap();
        let hex = hex.strip_prefix("0x").unwrap();
        assert_eq!(hex.len(), 128);
        let bytes: [u8; 64] = (0..64)
            .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let signature = solana_ed25519::ed_sigs::Signature::from(bytes);

        assert!(public_key.verify(&signature, b"test\x00\n").is_ok());
        assert!(public_key.verify(&signature, b"test").is_err());
    }
}
