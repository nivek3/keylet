//! Signing with the app's local signers. Signer keys never leave the app.
//!
//! Each `(app, signer, authTTL)` keeps one `keylet signer session` process, so
//! the app's signer authentication window applies across calls in this
//! process. Every response is verified against the reported public key.

use super::{BAD_RESPONSE, BridgeResult, drain, helper_command, hex, nonblocking, reported_error};
use k256::ecdsa::{
    RecoveryId, Signature as EcdsaSignature, VerifyingKey, signature::hazmat::PrehashVerifier,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::Write,
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};
use wallet_crypto::{Curve, valid_wallet_name};

/// Error returned when a signer request fails validation.
pub const INVALID_SIGNER: &str = "Invalid signer request";
/// Largest payload the app signs: a full Solana transaction message.
const MAX_PAYLOAD_BYTES: usize = 1232;
const TIMEOUT: Duration = Duration::from_secs(120);

/// A signature returned by a local signer and verified by this client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    /// The signer that produced the signature.
    pub name: String,
    /// The signer's curve.
    pub curve: Curve,
    /// Ed25519: 32-byte key. secp256k1: 33-byte compressed key. `0x` hex.
    pub public_key: String,
    /// Ed25519: 64-byte signature. secp256k1: 64-byte `r || s`, low-S. `0x` hex.
    pub signature: String,
    /// secp256k1 only: 0 or 1.
    pub recovery_id: Option<u8>,
    /// Ed25519 only: the Base58 public key (Solana address).
    pub address: Option<String>,
}

/// A local signer's public key, verified for its curve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignerPublicKey {
    /// The signer's name.
    pub name: String,
    /// The signer's curve.
    pub curve: Curve,
    /// Ed25519: 32-byte key. secp256k1: 33-byte compressed key. `0x` hex.
    pub public_key: String,
    /// Ed25519 only: the Base58 public key (Solana address).
    pub address: Option<String>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct SessionKey {
    helper: PathBuf,
    name: String,
    ttl: Option<u64>,
}

/// Most signer sessions kept per process. The least recently used one closes first.
const MAX_SESSIONS: usize = 8;

struct Session {
    process: HelperProcess,
    last_used: Instant,
}

// One session per signer and TTL; never caches signatures or failures.
static SESSIONS: OnceLock<Mutex<HashMap<SessionKey, Session>>> = OnceLock::new();

/// Bounds retained helper processes. Other sessions close when their app process
/// exited or when they have been idle longer than their `authTTL`: the window
/// starts at authentication, before the last use, so it has already ended. If
/// `keep` needs a new session, the least recently used ones close to make room.
fn prune(sessions: &mut HashMap<SessionKey, Session>, keep: &SessionKey, now: Instant) {
    sessions.retain(|key, session| {
        if key == keep {
            return true;
        }
        let running = matches!(session.process.child.try_wait(), Ok(None));
        let expired = key.ttl.is_some_and(|ttl| {
            now.saturating_duration_since(session.last_used) > Duration::from_secs(ttl)
        });
        running && !expired
    });
    if sessions.contains_key(keep) {
        return;
    }
    while sessions.len() >= MAX_SESSIONS {
        let Some(oldest) = sessions
            .iter()
            .min_by_key(|(_, session)| session.last_used)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        sessions.remove(&oldest);
    }
}

/// Signs `payload` with a local signer in the registered app.
///
/// Input: `{"name": string, "payload": "0x…" hex (1–1232 bytes), "authTTL"?: seconds}`.
///
/// # Errors
///
/// Returns [`INVALID_SIGNER`] for invalid input, or a stable code for discovery,
/// transport, app, or verification failures.
pub fn sign(input: &Value, cancelled: impl Fn() -> bool) -> BridgeResult<Signature> {
    sign_with(input, super::discovery::app_executable, cancelled)
}

/// Signs through a public test fixture in development builds.
///
/// # Errors
///
/// Returns the same errors as [`sign`].
#[cfg(debug_assertions)]
pub fn sign_for_test(
    input: &Value,
    executable: PathBuf,
    cancelled: impl Fn() -> bool,
) -> BridgeResult<Signature> {
    sign_with(input, || Ok(executable), cancelled)
}

/// Reads a local signer's public key. The app asks for authentication.
///
/// Input: `{"name": string}`.
///
/// # Errors
///
/// Returns [`INVALID_SIGNER`] for invalid input, or a stable code for discovery,
/// transport, or app failures.
pub fn public_key(input: &Value, cancelled: impl Fn() -> bool) -> BridgeResult<SignerPublicKey> {
    public_key_with(input, super::discovery::app_executable, cancelled)
}

/// Reads a public key through a public test fixture in development builds.
///
/// # Errors
///
/// Returns the same errors as [`public_key`].
#[cfg(debug_assertions)]
pub fn public_key_for_test(
    input: &Value,
    executable: PathBuf,
    cancelled: impl Fn() -> bool,
) -> BridgeResult<SignerPublicKey> {
    public_key_with(input, || Ok(executable), cancelled)
}

fn check_keys(input: &Value, allowed: &[&str]) -> BridgeResult<()> {
    let object = input.as_object().ok_or(INVALID_SIGNER)?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(INVALID_SIGNER);
    }
    Ok(())
}

fn signer_name(input: &Value) -> BridgeResult<String> {
    let name = input
        .get("name")
        .and_then(Value::as_str)
        .ok_or(INVALID_SIGNER)?;
    if !valid_wallet_name(name) {
        return Err(INVALID_SIGNER);
    }
    Ok(name.to_owned())
}

fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let digits = text.strip_prefix("0x")?.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16)?;
            let low = (pair[1] as char).to_digit(16)?;
            Some((high << 4 | low) as u8)
        })
        .collect()
}

fn validate_sign(input: &Value) -> BridgeResult<(String, Vec<u8>, Option<u64>)> {
    check_keys(input, &["name", "payload", "authTTL"])?;
    let name = signer_name(input)?;
    let payload = input
        .get("payload")
        .and_then(Value::as_str)
        .and_then(decode_hex)
        .filter(|payload| (1..=MAX_PAYLOAD_BYTES).contains(&payload.len()))
        .ok_or(INVALID_SIGNER)?;
    let ttl = match input.get("authTTL") {
        None => None, // Follow the app setting.
        Some(value) => Some(
            value
                .as_f64()
                .filter(|value| {
                    (0.0..=9_007_199_254_740_991.0).contains(value) && value.fract() == 0.0
                })
                .ok_or(INVALID_SIGNER)? as u64,
        ),
    };
    Ok((name, payload, ttl))
}

fn sign_with(
    input: &Value,
    discover: impl FnOnce() -> BridgeResult<PathBuf>,
    cancelled: impl Fn() -> bool,
) -> BridgeResult<Signature> {
    let (name, payload, ttl) = validate_sign(input)?;
    let key = SessionKey {
        helper: discover()?,
        name,
        ttl,
    };
    let mut sessions = SESSIONS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .map_err(|_| "KEYCHAIN_ERROR")?;
    if cancelled() {
        return Err("CANCELLED");
    }
    prune(&mut sessions, &key, Instant::now());
    let alive = match sessions.get_mut(&key) {
        Some(session) => session
            .process
            .child
            .try_wait()
            .map_err(|_| "TRANSPORT_ERROR")?
            .is_none(),
        None => false,
    };
    if !alive {
        sessions.remove(&key);
        let mut args = vec!["signer".to_owned(), "session".to_owned(), key.name.clone()];
        if let Some(ttl) = key.ttl {
            args.extend(["--ttl".to_owned(), ttl.to_string()]);
        }
        let process = HelperProcess::start(&key.helper, &args, true)?;
        sessions.insert(
            key.clone(),
            Session {
                process,
                last_used: Instant::now(),
            },
        );
    }
    let session = sessions.get_mut(&key).ok_or("TRANSPORT_ERROR")?;
    let frame = json!({ "payload": hex(&payload) });
    let result = session
        .process
        .write_line(&frame.to_string())
        .and_then(|()| session.process.read_line(TIMEOUT, &cancelled))
        .and_then(|(line, err)| parse_signature(&key.name, &payload, &line, &err));
    match result {
        Ok(_) => session.last_used = Instant::now(),
        // The app ends a session on any error; start fresh on the next call.
        Err(_) => {
            sessions.remove(&key);
        }
    }
    result
}

fn public_key_with(
    input: &Value,
    discover: impl FnOnce() -> BridgeResult<PathBuf>,
    cancelled: impl Fn() -> bool,
) -> BridgeResult<SignerPublicKey> {
    check_keys(input, &["name"])?;
    let name = signer_name(input)?;
    let helper = discover()?;
    let args = ["signer".to_owned(), "public-key".to_owned(), name.clone()];
    let mut process = HelperProcess::start(&helper, &args, false)?;
    let (line, err) = process.read_line(TIMEOUT, &cancelled)?;
    let response = parse_object(&line, &err)?;
    let (curve, public_key, address) =
        check_public_key(&name, &response, &["name", "curve", "publicKey", "address"])?;
    Ok(SignerPublicKey {
        name,
        curve,
        public_key: hex(&public_key),
        address,
    })
}

fn parse_object(line: &[u8], err: &[u8]) -> BridgeResult<serde_json::Map<String, Value>> {
    if !err.iter().all(u8::is_ascii_whitespace) {
        return Err(BAD_RESPONSE);
    }
    match serde_json::from_slice(line).map_err(|_| BAD_RESPONSE)? {
        Value::Object(object) => Ok(object),
        _ => Err(BAD_RESPONSE),
    }
}

/// Checks the shared public fields and returns the curve, raw public key, and address.
fn check_public_key(
    name: &str,
    response: &serde_json::Map<String, Value>,
    allowed: &[&str],
) -> BridgeResult<(Curve, Vec<u8>, Option<String>)> {
    if response.get("name").and_then(Value::as_str) != Some(name) {
        return Err(BAD_RESPONSE);
    }
    let curve = match response.get("curve").and_then(Value::as_str) {
        Some("ed25519") => Curve::Ed25519,
        Some("secp256k1") => Curve::Secp256k1,
        _ => return Err(BAD_RESPONSE),
    };
    let public_key = response
        .get("publicKey")
        .and_then(Value::as_str)
        .and_then(decode_hex)
        .ok_or(BAD_RESPONSE)?;
    let address = match curve {
        Curve::Ed25519 => {
            let key: [u8; 32] = public_key.as_slice().try_into().map_err(|_| BAD_RESPONSE)?;
            solana_ed25519::ed_sigs::VerificationKey::try_from(key).map_err(|_| BAD_RESPONSE)?;
            let address = bs58::encode(key).into_string();
            if response.get("address").and_then(Value::as_str) != Some(&address) {
                return Err(BAD_RESPONSE);
            }
            Some(address)
        }
        Curve::Secp256k1 => {
            if public_key.len() != 33 || response.contains_key("address") {
                return Err(BAD_RESPONSE);
            }
            VerifyingKey::from_sec1_bytes(&public_key).map_err(|_| BAD_RESPONSE)?;
            None
        }
    };
    if response.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(BAD_RESPONSE);
    }
    Ok((curve, public_key, address))
}

fn parse_signature(name: &str, payload: &[u8], line: &[u8], err: &[u8]) -> BridgeResult<Signature> {
    let response = parse_object(line, err)?;
    let allowed = [
        "name",
        "curve",
        "publicKey",
        "address",
        "signature",
        "recoveryId",
    ];
    let (curve, public_key, address) = check_public_key(name, &response, &allowed)?;
    let signature: [u8; 64] = response
        .get("signature")
        .and_then(Value::as_str)
        .and_then(decode_hex)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(BAD_RESPONSE)?;
    let recovery_id = match curve {
        Curve::Ed25519 => {
            if response.contains_key("recoveryId") {
                return Err(BAD_RESPONSE);
            }
            let key: [u8; 32] = public_key.as_slice().try_into().map_err(|_| BAD_RESPONSE)?;
            solana_ed25519::ed_sigs::VerificationKey::try_from(key)
                .and_then(|key| {
                    key.verify(
                        &solana_ed25519::ed_sigs::Signature::from(signature),
                        payload,
                    )
                })
                .map_err(|_| BAD_RESPONSE)?;
            None
        }
        Curve::Secp256k1 => {
            let recovery = response
                .get("recoveryId")
                .and_then(Value::as_u64)
                .filter(|id| *id <= 1)
                .ok_or(BAD_RESPONSE)? as u8;
            let key = VerifyingKey::from_sec1_bytes(&public_key).map_err(|_| BAD_RESPONSE)?;
            let ecdsa = EcdsaSignature::from_slice(&signature).map_err(|_| BAD_RESPONSE)?;
            // Bitcoin and Ethereum both require low-S signatures.
            if payload.len() != 32 || ecdsa.normalize_s().is_some() {
                return Err(BAD_RESPONSE);
            }
            key.verify_prehash(payload, &ecdsa)
                .map_err(|_| BAD_RESPONSE)?;
            let recovered = VerifyingKey::recover_from_prehash(
                payload,
                &ecdsa,
                RecoveryId::from_byte(recovery).ok_or(BAD_RESPONSE)?,
            )
            .map_err(|_| BAD_RESPONSE)?;
            if recovered != key {
                return Err(BAD_RESPONSE);
            }
            Some(recovery)
        }
    };
    Ok(Signature {
        name: name.to_owned(),
        curve,
        public_key: hex(&public_key),
        signature: hex(&signature),
        recovery_id,
        address,
    })
}

/// A helper process that answers one JSON line per request on stdout.
struct HelperProcess {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    stderr: ChildStderr,
}

impl Drop for HelperProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl HelperProcess {
    fn start(helper: &Path, args: &[String], interactive: bool) -> BridgeResult<Self> {
        let mut command = helper_command(helper);
        command
            .args(args)
            .stdin(if interactive {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|_| "HELPER_START_FAILED")?;
        let streams = (|| {
            let stdout = child.stdout.take().ok_or("TRANSPORT_ERROR")?;
            let stderr = child.stderr.take().ok_or("TRANSPORT_ERROR")?;
            for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
                nonblocking(fd)?;
            }
            Ok((child.stdin.take(), stdout, stderr))
        })();
        match streams {
            Ok((stdin, stdout, stderr)) => Ok(Self {
                child,
                stdin,
                stdout,
                stderr,
            }),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(error)
            }
        }
    }

    fn write_line(&mut self, line: &str) -> BridgeResult<()> {
        let stdin = self.stdin.as_mut().ok_or("TRANSPORT_ERROR")?;
        writeln!(stdin, "{line}")
            .and_then(|()| stdin.flush())
            .map_err(|_| "TRANSPORT_ERROR")
    }

    /// Reads exactly one newline-terminated response and any stderr seen so far.
    fn read_line(
        &mut self,
        timeout: Duration,
        cancelled: &impl Fn() -> bool,
    ) -> BridgeResult<(Vec<u8>, Vec<u8>)> {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let deadline = Instant::now() + timeout;
        loop {
            if cancelled() {
                return Err("CANCELLED");
            }
            let out_done = drain(&mut self.stdout, &mut out, 4096)?;
            let err_done = drain(&mut self.stderr, &mut err, 1024)?;
            if let Some(end) = out.iter().position(|byte| *byte == b'\n') {
                // One request yields one line; anything after it is a protocol error.
                if end + 1 != out.len() {
                    return Err(BAD_RESPONSE);
                }
                out.truncate(end);
                return Ok((out, err));
            }
            if let Some(status) = self.child.try_wait().map_err(|_| "TRANSPORT_ERROR")?
                && out_done
                && err_done
            {
                return Err(if status.success() {
                    BAD_RESPONSE
                } else {
                    reported_error(&err)
                });
            }
            if Instant::now() >= deadline {
                return Err("AUTHENTICATION_TIMEOUT");
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k256::ecdsa::SigningKey as EcdsaKey;
    use solana_ed25519::ed_sigs::SigningKey as EdKey;
    use std::{fs, os::unix::fs::PermissionsExt};

    const DIGEST: [u8; 32] = [7; 32];

    fn ed_response(name: &str, payload: &[u8]) -> Value {
        let key = EdKey::from_bytes(&[17; 32]);
        let public: [u8; 32] = key.verification_key().into();
        json!({
            "name": name,
            "curve": "ed25519",
            "publicKey": hex(&public),
            "address": bs58::encode(public).into_string(),
            "signature": hex(&key.sign(payload).to_bytes()),
        })
    }

    fn secp_response(name: &str, digest: &[u8]) -> Value {
        let key = EcdsaKey::from_slice(&[17; 32]).unwrap();
        let (signature, recovery) = key.sign_prehash_recoverable(digest).unwrap();
        let signature = signature.normalize_s().unwrap_or(signature);
        json!({
            "name": name,
            "curve": "secp256k1",
            "publicKey": hex(key.verifying_key().to_encoded_point(true).as_bytes()),
            "signature": hex(&signature.to_bytes()),
            "recoveryId": recovery.to_byte(),
        })
    }

    fn parse(name: &str, payload: &[u8], response: &Value) -> BridgeResult<Signature> {
        parse_signature(name, payload, response.to_string().as_bytes(), b"")
    }

    #[test]
    fn sign_requests_are_validated_before_discovery() {
        let valid = json!({"name": "agent", "payload": "0x00ff"});
        assert_eq!(
            validate_sign(&valid).unwrap(),
            ("agent".into(), vec![0, 255], None)
        );
        let mut with_ttl = valid;
        with_ttl["authTTL"] = json!(300);
        assert_eq!(validate_sign(&with_ttl).unwrap().2, Some(300));
        let largest = format!("0x{}", "ab".repeat(MAX_PAYLOAD_BYTES));
        assert!(validate_sign(&json!({"name": "agent", "payload": largest})).is_ok());
        for input in [
            json!(null),
            json!({"name": "agent"}),
            json!({"payload": "0x00"}),
            json!({"name": "bad\n", "payload": "0x00"}),
            json!({"name": "agent", "payload": "0x"}),
            json!({"name": "agent", "payload": "00"}),
            json!({"name": "agent", "payload": "0x0"}),
            json!({"name": "agent", "payload": "0xzz"}),
            json!({"name": "agent", "payload": [0]}),
            json!({"name": "agent", "payload": format!("0x{}", "ab".repeat(MAX_PAYLOAD_BYTES + 1))}),
            json!({"name": "agent", "payload": "0x00", "authTTL": -1}),
            json!({"name": "agent", "payload": "0x00", "authTTL": 0.5}),
            json!({"name": "agent", "payload": "0x00", "authTTL": "1"}),
            json!({"name": "agent", "payload": "0x00", "helperPath": "/tmp/x"}),
        ] {
            assert_eq!(validate_sign(&input).err(), Some(INVALID_SIGNER), "{input}");
        }
        let discovered = std::cell::Cell::new(false);
        let result = sign_with(
            &json!({"name": "agent", "payload": "0x"}),
            || {
                discovered.set(true);
                Err("unreachable")
            },
            || false,
        );
        assert_eq!(result.err(), Some(INVALID_SIGNER));
        assert!(!discovered.get());
    }

    #[test]
    fn valid_signatures_for_both_curves_are_accepted() {
        let ed = parse("sol", b"message", &ed_response("sol", b"message")).unwrap();
        assert_eq!(ed.curve, Curve::Ed25519);
        assert!(ed.address.is_some() && ed.recovery_id.is_none());
        let secp = parse("evm", &DIGEST, &secp_response("evm", &DIGEST)).unwrap();
        assert_eq!(secp.curve, Curve::Secp256k1);
        assert!(secp.address.is_none() && secp.recovery_id.is_some());
        assert_eq!(secp.public_key.len(), 2 + 66);
    }

    #[test]
    fn tampered_or_mismatched_responses_fail_closed() {
        let ed = ed_response("sol", b"message");
        let secp = secp_response("evm", &DIGEST);
        let mut cases: Vec<(&str, Vec<u8>, Value)> = vec![
            ("sol", b"other".to_vec(), ed.clone()),
            ("other", b"message".to_vec(), ed.clone()),
            ("evm", [8; 32].to_vec(), secp.clone()),
            ("evm", [7; 31].to_vec(), secp.clone()),
        ];
        let edited = |base: &Value, key: &str, value: Value| {
            let mut response = base.clone();
            if value.is_null() {
                response.as_object_mut().unwrap().remove(key);
            } else {
                response[key] = value;
            }
            response
        };
        for (key, value) in [
            ("address", json!("wrong")),
            ("address", Value::Null),
            ("recoveryId", json!(0)),
            ("curve", json!("secp256k1")),
            ("signature", json!(hex(&[0; 64]))),
            ("extra", json!(true)),
        ] {
            cases.push(("sol", b"message".to_vec(), edited(&ed, key, value)));
        }
        // Replace s with n - s: still a valid signature, but high-S.
        let low_s = decode_hex(secp["signature"].as_str().unwrap()).unwrap();
        let (r, s) = EcdsaSignature::from_slice(&low_s).unwrap().split_scalars();
        let high_s = EcdsaSignature::from_scalars(r, -*s).unwrap();
        let flipped = 1 - secp["recoveryId"].as_u64().unwrap();
        for (key, value) in [
            ("recoveryId", json!(2)),
            ("recoveryId", Value::Null),
            ("recoveryId", json!(flipped)),
            ("address", json!("0x00")),
            ("publicKey", json!(hex(&[2; 33]))),
            ("signature", json!(hex(&high_s.to_bytes()))),
        ] {
            cases.push(("evm", DIGEST.to_vec(), edited(&secp, key, value)));
        }
        for (name, payload, response) in cases {
            assert_eq!(
                parse(name, &payload, &response).err(),
                Some(BAD_RESPONSE),
                "{response}"
            );
        }
        assert_eq!(
            parse_signature("sol", b"message", ed.to_string().as_bytes(), b"warning").err(),
            Some(BAD_RESPONSE)
        );
    }

    fn fixture(script: &str) -> (PathBuf, PathBuf) {
        let root = crate::tests::folder();
        let helper = root.join("helper.sh");
        fs::write(&helper, format!("#!/bin/sh\n{script}")).unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        (root, helper)
    }

    // Session tests share the process-wide cache, and eviction is global.
    static SESSION_TESTS: Mutex<()> = Mutex::new(());

    fn helper_sessions(helper: &Path) -> Vec<Option<u64>> {
        let sessions = SESSIONS.get().unwrap().lock().unwrap();
        let mut ttls: Vec<_> = sessions
            .keys()
            .filter(|key| key.helper == helper)
            .map(|key| key.ttl)
            .collect();
        ttls.sort();
        ttls
    }

    #[test]
    fn retained_sessions_are_bounded_and_idle_ones_close_after_their_ttl() {
        let _serial = SESSION_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let response = secp_response("bounded", &DIGEST);
        let (root, helper) = fixture(&format!(
            "echo \"$*\" >> \"$0.starts\"\nwhile read line; do echo '{response}'; done\n"
        ));
        let sign = |ttl: u64| {
            let input = json!({"name": "bounded", "payload": hex(&DIGEST), "authTTL": ttl});
            sign_for_test(&input, helper.clone(), || false).map(drop)
        };
        let starts = || {
            fs::read_to_string(helper.with_extension("sh.starts"))
                .unwrap()
                .lines()
                .count()
        };
        for ttl in 1000..=(1000 + MAX_SESSIONS as u64) {
            sign(ttl).unwrap();
        }
        // The least recently used session (1000) closed to make room.
        let expected: Vec<_> = (1001..=(1000 + MAX_SESSIONS as u64)).map(Some).collect();
        assert_eq!(helper_sessions(&helper), expected);
        assert_eq!(starts(), MAX_SESSIONS + 1);
        sign(1000 + MAX_SESSIONS as u64).unwrap();
        assert_eq!(starts(), MAX_SESSIONS + 1, "an active session is reused");
        sign(1000).unwrap();
        assert_eq!(starts(), MAX_SESSIONS + 2);
        assert!(!helper_sessions(&helper).contains(&Some(1001)));

        // A session idle longer than its authTTL can no longer skip a prompt.
        sign(1).unwrap();
        thread::sleep(Duration::from_millis(1100));
        sign(1000).unwrap();
        assert!(!helper_sessions(&helper).contains(&Some(1)));
        assert!(helper_sessions(&helper).contains(&Some(1000)));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sessions_are_reused_per_signer_and_ttl_and_restart_after_errors() {
        let _serial = SESSION_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let response = secp_response("reuse", &DIGEST);
        let (root, helper) = fixture(&format!(
            "echo \"$*\" >> \"$0.starts\"\nwhile read line; do\n  case \"$line\" in *0x0000*) echo CANCELLED >&2; exit 1;; esac\n  echo '{response}'\ndone\n"
        ));
        let request = |ttl: Option<u64>, payload: &[u8]| {
            let mut input = json!({"name": "reuse", "payload": hex(payload)});
            if let Some(ttl) = ttl {
                input["authTTL"] = json!(ttl);
            }
            sign_for_test(&input, helper.clone(), || false)
        };
        let starts = || fs::read_to_string(helper.with_extension("sh.starts")).unwrap();
        for _ in 0..3 {
            assert_eq!(
                request(Some(60), &DIGEST).unwrap().recovery_id,
                response["recoveryId"].as_u64().map(|id| id as u8)
            );
        }
        assert_eq!(starts(), "signer session reuse --ttl 60\n");
        request(None, &DIGEST).unwrap();
        assert_eq!(starts().lines().last(), Some("signer session reuse"));
        assert_eq!(request(Some(60), &[0; 32]).err(), Some("CANCELLED"));
        request(Some(60), &DIGEST).unwrap();
        assert_eq!(starts().lines().count(), 3);
        // A valid signature for another payload is rejected.
        assert_eq!(request(Some(60), &[9; 32]).err(), Some(BAD_RESPONSE));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn public_key_reads_one_response_and_reports_app_errors() {
        let response = ed_response("sol", b"x");
        let mut public = response.as_object().unwrap().clone();
        public.remove("signature");
        let (root, helper) = fixture(&format!(
            "[ \"$*\" = 'signer public-key sol' ] || {{ echo INVALID_INPUT >&2; exit 1; }}\necho '{}'\n",
            Value::Object(public.clone())
        ));
        let key = public_key_for_test(&json!({"name": "sol"}), helper.clone(), || false).unwrap();
        assert_eq!(
            (key.curve, key.public_key.as_str()),
            (Curve::Ed25519, public["publicKey"].as_str().unwrap())
        );
        assert_eq!(
            public_key_for_test(&json!({"name": "other"}), helper, || false).err(),
            Some("INVALID_INPUT")
        );
        assert_eq!(
            public_key_for_test(&json!({"name": "sol", "x": 1}), PathBuf::new(), || false).err(),
            Some(INVALID_SIGNER)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stalled_signer_times_out_and_cancellation_stops_waiting() {
        let (root, helper) = fixture("/bin/sleep 2\n");
        let args = ["signer".to_owned()];
        let start = Instant::now();
        let mut process = HelperProcess::start(&helper, &args, true).unwrap();
        assert_eq!(
            process
                .read_line(Duration::from_millis(40), &|| false)
                .err(),
            Some("AUTHENTICATION_TIMEOUT")
        );
        assert_eq!(
            process.read_line(Duration::from_secs(5), &|| true).err(),
            Some("CANCELLED")
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        fs::remove_dir_all(root).unwrap();
    }
}
