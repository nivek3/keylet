use crate::{
    emit,
    macos::{self, AuthContext, AuthenticationSession, Vault},
    model::{self, Curve, Failure, Result, SignerKey},
};
use sha3::{Digest, Sha3_256};
use solana_ed25519::ed_sigs::SigningKey;
use std::{
    io::Read,
    os::fd::FromRawFd,
    time::{Duration, Instant},
};

/// Largest payload: a full Solana transaction message for Ed25519.
const MAX_PAYLOAD_BYTES: usize = 1232;
/// secp256k1 signers sign a 32-byte prehash (Ethereum or Bitcoin sighash).
const DIGEST_BYTES: usize = 32;
/// A session frame `{"payload":"0x…"}` holding the largest payload.
const MAX_FRAME_BYTES: usize = 2 * MAX_PAYLOAD_BYTES + 64;

fn parse_curve(text: &str) -> Result<Curve> {
    match text {
        "ed25519" => Ok(Curve::Ed25519),
        "secp256k1" => Ok(Curve::Secp256k1),
        _ => Err(Failure::Input),
    }
}

/// Parse `--key value` pairs, rejecting unknown and repeated keys.
fn parse_options<'a>(args: &'a [String], allowed: &[&str]) -> Result<Vec<(&'a str, &'a str)>> {
    if !args.len().is_multiple_of(2) {
        return Err(Failure::Input);
    }
    let mut options: Vec<(&str, &str)> = Vec::new();
    for pair in args.chunks(2) {
        let key = pair[0].strip_prefix("--").ok_or(Failure::Input)?;
        if !allowed.contains(&key) || options.iter().any(|(seen, _)| *seen == key) {
            return Err(Failure::Input);
        }
        options.push((key, &pair[1]));
    }
    Ok(options)
}

fn option<'a>(options: &[(&str, &'a str)], key: &str) -> Option<&'a str> {
    options
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| *value)
}

fn parse_seconds(text: &str) -> Result<u64> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Failure::Input);
    }
    text.parse().map_err(|_| Failure::Input)
}

fn decode_hex(text: &str) -> Result<Vec<u8>> {
    let digits = text.strip_prefix("0x").ok_or(Failure::Input)?.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return Err(Failure::Input);
    }
    let value = |digit: u8| (digit as char).to_digit(16).ok_or(Failure::Input);
    digits
        .chunks(2)
        .map(|pair| Ok((value(pair[0])? << 4 | value(pair[1])?) as u8))
        .collect()
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

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionRequest {
    payload: String,
}

fn parse_frame(frame: &[u8]) -> Result<Vec<u8>> {
    let request: SessionRequest = serde_json::from_slice(frame).map_err(|_| Failure::Input)?;
    let payload = decode_hex(&request.payload)?;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Failure::Input);
    }
    Ok(payload)
}

fn public_info(name: &str, key: &SignerKey) -> Result<serde_json::Value> {
    Ok(match key.curve {
        Curve::Ed25519 => {
            let public_key: [u8; 32] = SigningKey::from_bytes(&key.secret)
                .verification_key()
                .into();
            serde_json::json!({
                "name": name,
                "curve": key.curve.name(),
                "publicKey": model::hex(&public_key),
                "address": bs58::encode(public_key).into_string(),
            })
        }
        Curve::Secp256k1 => {
            let public_key =
                wallet_crypto::secp256k1_public_key(&key.secret).ok_or(Failure::Corrupt)?;
            serde_json::json!({
                "name": name,
                "curve": key.curve.name(),
                "publicKey": model::hex(&public_key),
            })
        }
    })
}

fn signature_response(name: &str, key: &SignerKey, payload: &[u8]) -> Result<serde_json::Value> {
    let mut response = public_info(name, key)?;
    match key.curve {
        Curve::Ed25519 => {
            if payload.is_empty() || payload.len() > MAX_PAYLOAD_BYTES {
                return Err(Failure::Input);
            }
            let signature = SigningKey::from_bytes(&key.secret).sign(payload);
            response["signature"] = model::hex(&signature.to_bytes()).into();
        }
        Curve::Secp256k1 => {
            let digest: &[u8; DIGEST_BYTES] = payload.try_into().map_err(|_| Failure::Input)?;
            let (signature, recovery) = wallet_crypto::secp256k1_sign_prehash(&key.secret, digest)
                .ok_or(Failure::Corrupt)?;
            response["signature"] = model::hex(&signature).into();
            response["recoveryId"] = recovery.into();
        }
    }
    Ok(response)
}

/// Short payloads, such as a 32-byte digest, are shown in full; longer ones by digest.
fn describe(payload: &[u8]) -> String {
    if payload.len() <= DIGEST_BYTES {
        format!("Payload: {}", model::hex(payload))
    } else {
        format!("SHA3-256: {}", model::hex(&Sha3_256::digest(payload)))
    }
}

fn sign_reason(name: &str, payload: &[u8]) -> String {
    format!(
        "sign {} unverified bytes with local signer \"{name}\"\n{}",
        payload.len(),
        describe(payload)
    )
}

fn duration(seconds: u64) -> String {
    match seconds {
        60 => "1 minute".into(),
        s if s % 60 == 0 => format!("{} minutes", s / 60),
        1 => "1 second".into(),
        s => format!("{s} seconds"),
    }
}

fn session_reason(name: &str, payload: &[u8], ttl: u64) -> String {
    format!(
        "start a signing session with local signer \"{name}\". For {}, this process can sign any payload without asking again.\nFirst request: {} bytes, {}",
        duration(ttl),
        payload.len(),
        describe(payload)
    )
}

fn session(vault: &Vault, name: &str, requested_ttl: u64) -> Result<()> {
    macos::initialize_accessory()?;
    let mut auth = AuthenticationSession::default();
    // Read unbuffered so polling the descriptor sees every byte not yet consumed.
    // SAFETY: fd 0 is this process's stdin and nothing else here reads it; ManuallyDrop keeps the File from closing it.
    let mut input = std::mem::ManuallyDrop::new(unsafe { std::fs::File::from_raw_fd(0) });
    while let Some(frame) = crate::read_frame(&mut *input, &mut auth, Some(0), MAX_FRAME_BYTES)? {
        let payload = parse_frame(&frame)?;
        objc2::rc::autoreleasepool(|_| -> Result<()> {
            // The saved setting is an upper bound and also applies to running sessions.
            let ttl = requested_ttl.min(vault.signer_authentication_ttl()?);
            let reason = if ttl == 0 {
                sign_reason(name, &payload)
            } else {
                session_reason(name, &payload, ttl)
            };
            let context = auth.context(&reason, Duration::from_secs(ttl), Instant::now());
            let key = vault.read_signer(name, context);
            auth.complete(key.is_ok(), Instant::now());
            emit(&signature_response(name, &key?, &payload)?)
        })?;
    }
    Ok(())
}

pub fn run(vault: &Vault, operation: &str, name: &str, args: &[String]) -> Result<()> {
    model::validate_name(name)?;
    match operation {
        "create" => {
            let options = parse_options(args, &["curve"])?;
            let curve = option(&options, "curve").map_or(Ok(Curve::Ed25519), parse_curve)?;
            let key = SignerKey {
                curve,
                secret: macos::random_secret::<32>()?,
            };
            vault.insert_signer(name, &key)?;
            emit(&public_info(name, &key)?)
        }
        "derive" => {
            let options = parse_options(args, &["wallet", "curve", "path"])?;
            let wallet = option(&options, "wallet").ok_or(Failure::Input)?;
            let curve = parse_curve(option(&options, "curve").ok_or(Failure::Input)?)?;
            let path = option(&options, "path").ok_or(Failure::Input)?;
            model::validate_name(wallet)?;
            if !curve.valid_signer_path(path) {
                return Err(Failure::Input);
            }
            macos::initialize_accessory()?;
            let context = AuthContext::new(
                &format!(
                    "copy a key from wallet \"{wallet}\" into local signer \"{name}\"\nCurve: {}\nPath: {path}",
                    curve.name()
                ),
                false,
            );
            let key = model::derive_signer(&vault.read(wallet, &context)?, curve, path)?;
            vault.insert_signer(name, &key)?;
            let mut response = public_info(name, &key)?;
            response["wallet"] = wallet.into();
            response["path"] = path.into();
            emit(&response)
        }
        "public-key" => {
            parse_options(args, &[])?;
            macos::initialize_accessory()?;
            let context = AuthContext::new(
                &format!("read the public key of local signer \"{name}\""),
                false,
            );
            emit(&public_info(name, &vault.read_signer(name, &context)?)?)
        }
        "sign" => {
            parse_options(args, &[])?;
            let payload = read_payload(std::io::stdin().lock())?;
            macos::initialize_accessory()?;
            let context = AuthContext::new(&sign_reason(name, &payload), false);
            let key = vault.read_signer(name, &context)?;
            emit(&signature_response(name, &key, &payload)?)
        }
        "session" => {
            let options = parse_options(args, &["ttl"])?;
            // Without --ttl, follow the app setting.
            let ttl = option(&options, "ttl").map_or(Ok(u64::MAX), parse_seconds)?;
            session(vault, name, ttl)
        }
        "delete" => {
            parse_options(args, &[])?;
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
    use zeroize::Zeroizing;

    fn bytes_from_hex(value: &serde_json::Value) -> Vec<u8> {
        decode_hex(value.as_str().unwrap()).unwrap()
    }

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
        let key = SignerKey {
            curve: Curve::Ed25519,
            secret: Zeroizing::new([17; 32]),
        };
        let payload = read_payload(&b"test\x00\n"[..]).unwrap();
        let response = signature_response("agent", &key, &payload).unwrap();
        assert_eq!(response["name"], "agent");
        assert_eq!(response["curve"], "ed25519");

        let address = response["address"].as_str().unwrap();
        let public_key: [u8; 32] = bs58::decode(address)
            .into_vec()
            .unwrap()
            .try_into()
            .unwrap();
        assert_eq!(bytes_from_hex(&response["publicKey"]), public_key);
        let public_key = solana_ed25519::ed_sigs::VerificationKey::try_from(public_key).unwrap();

        let signature: [u8; 64] = bytes_from_hex(&response["signature"]).try_into().unwrap();
        let signature = solana_ed25519::ed_sigs::Signature::from(signature);

        assert!(public_key.verify(&signature, b"test\x00\n").is_ok());
        assert!(public_key.verify(&signature, b"test").is_err());
    }

    #[test]
    fn secp256k1_signs_exactly_one_digest_and_reports_recovery() {
        let key = SignerKey {
            curve: Curve::Secp256k1,
            secret: Zeroizing::new([17; 32]),
        };
        let digest = [7; 32];
        let response = signature_response("evm", &key, &digest).unwrap();
        assert_eq!(response["curve"], "secp256k1");
        assert!(response.get("address").is_none());
        let public_key = bytes_from_hex(&response["publicKey"]);
        assert_eq!(public_key.len(), 33);
        let (expected, recovery) =
            wallet_crypto::secp256k1_sign_prehash(&key.secret, &digest).unwrap();
        assert_eq!(bytes_from_hex(&response["signature"]), expected);
        assert_eq!(response["recoveryId"], recovery);
        for payload in [&[7; 31][..], &[7; 33][..]] {
            assert_eq!(
                signature_response("evm", &key, payload),
                Err(Failure::Input)
            );
        }
    }

    #[test]
    fn signer_keys_round_trip_with_a_curve_tag() {
        for (curve, tag) in [(Curve::Ed25519, 0), (Curve::Secp256k1, 1)] {
            let key = SignerKey {
                curve,
                secret: Zeroizing::new([9; 32]),
            };
            let encoded = key.encode();
            assert_eq!(encoded[0], tag);
            assert_eq!(&encoded[1..], &[9; 32]);
            let decoded = SignerKey::decode(&encoded).unwrap();
            assert_eq!((decoded.curve, *decoded.secret), (curve, [9; 32]));
        }
        // Untagged 32-byte data, unknown tags, and wrong lengths are rejected.
        for invalid in [
            &[][..],
            &[9; 32][..],
            &[0; 32][..],
            &[2; 33][..],
            &[1; 34][..],
        ] {
            assert!(SignerKey::decode(invalid).is_err());
        }
        // A zero scalar is not a valid secp256k1 key.
        let mut zero = vec![1];
        zero.extend([0; 32]);
        assert!(SignerKey::decode(&zero).is_err());
    }

    #[test]
    fn session_frames_carry_one_hex_payload() {
        assert_eq!(parse_frame(br#"{"payload":"0x00ff"}"#).unwrap(), [0, 255]);
        let largest = format!(r#"{{"payload":"0x{}"}}"#, "ab".repeat(MAX_PAYLOAD_BYTES));
        assert!(largest.len() <= MAX_FRAME_BYTES);
        assert_eq!(
            parse_frame(largest.as_bytes()).unwrap().len(),
            MAX_PAYLOAD_BYTES
        );
        for invalid in [
            &br#"{"payload":"0x"}"#[..],
            br#"{"payload":"00ff"}"#,
            br#"{"payload":"0x0ff"}"#,
            br#"{"payload":"0xzz"}"#,
            br#"{"payload":"0x00","extra":1}"#,
            br"{}",
        ] {
            assert_eq!(parse_frame(invalid), Err(Failure::Input));
        }
        let too_large = format!(
            r#"{{"payload":"0x{}"}}"#,
            "ab".repeat(MAX_PAYLOAD_BYTES + 1)
        );
        assert_eq!(parse_frame(too_large.as_bytes()), Err(Failure::Input));
    }

    #[test]
    fn options_reject_unknown_repeated_and_dangling_keys() {
        let args = |list: &[&str]| {
            list.iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>()
        };
        let given = args(&["--curve", "secp256k1"]);
        let options = parse_options(&given, &["curve"]).unwrap();
        assert_eq!(option(&options, "curve"), Some("secp256k1"));
        assert_eq!(option(&options, "path"), None);
        for invalid in [
            args(&["--curve"]),
            args(&["curve", "ed25519"]),
            args(&["--path", "m/0"]),
            args(&["--curve", "ed25519", "--curve", "secp256k1"]),
        ] {
            assert!(parse_options(&invalid, &["curve"]).is_err());
        }
        assert_eq!(parse_curve("secp256k1"), Ok(Curve::Secp256k1));
        assert!(parse_curve("bitcoin").is_err());
        assert_eq!(parse_seconds("300"), Ok(300));
        for invalid in ["", "-1", "1.5", "+3", "99999999999999999999"] {
            assert!(parse_seconds(invalid).is_err());
        }
    }

    #[test]
    fn prompts_show_short_payloads_and_the_session_window() {
        let digest = [0xab; 32];
        assert!(sign_reason("evm", &digest).contains(&model::hex(&digest)));
        assert!(sign_reason("sol", &[0; 33]).contains("SHA3-256: 0x"));
        let reason = session_reason("evm", &digest, 300);
        assert!(reason.contains("For 5 minutes"));
        assert!(reason.contains(&model::hex(&digest)));
        assert_eq!(duration(60), "1 minute");
        assert_eq!(duration(90), "90 seconds");
    }
}
