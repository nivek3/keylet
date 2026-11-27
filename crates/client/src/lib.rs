//! Shared client for the signed macOS app's private key request protocol.

use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::{Read, Write},
    os::{
        fd::{AsRawFd, RawFd},
        unix::{net::UnixStream, process::CommandExt},
    },
    path::{Path, PathBuf},
    process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio},
    sync::{Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};
use wallet_crypto::error::HelperError;
use wallet_crypto::{Curve, encode_hex, valid_wallet_name};
use zeroize::Zeroizing;

mod discovery;
mod error;
mod protocol;
pub mod signer;

pub use error::ClientError;
use error::{
    ClientError::{InvalidHelperResponse, InvalidWalletSelection},
    reported_error,
};

type BridgeResult<T> = Result<T, ClientError>;

#[derive(Clone, Hash, PartialEq, Eq)]
struct Selection {
    helper: PathBuf,
    name: String,
    curve: Curve,
    path: String,
    authentication_ttl: u64,
}

/// A validated key pair returned by the app.
pub struct KeyPair {
    private: Zeroizing<[u8; 32]>,
    public: String,
}

impl std::fmt::Debug for KeyPair {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the private key.
        formatter
            .debug_struct("KeyPair")
            .field("public", &self.public)
            .finish_non_exhaustive()
    }
}

impl KeyPair {
    /// Returns the public and private keys as lowercase `0x` hex strings.
    pub fn into_hex(self) -> (String, String) {
        (self.public, encode_hex(&*self.private))
    }
}

// One private pipe session per client process; no key or failure cache.
static SESSION: OnceLock<Mutex<Option<AppSession>>> = OnceLock::new();

/// Validates the selection and requests its key pair from the registered app.
///
/// The cancellation predicate runs while holding the session lock.
///
/// # Errors
///
/// Returns a client error for invalid input, discovery, transport, or app failures.
pub fn key_pair(input: &Value, cancelled: impl Fn() -> bool) -> BridgeResult<KeyPair> {
    request(input, discovery::app_executable, cancelled)
}

/// Sends a request to a public test fixture in development builds.
///
/// # Errors
///
/// Returns the same errors as [`key_pair`] for invalid input or helper failures.
#[cfg(debug_assertions)]
pub fn key_pair_for_test(
    input: &Value,
    executable: PathBuf,
    cancelled: impl Fn() -> bool,
) -> BridgeResult<KeyPair> {
    request(input, || Ok(executable), cancelled)
}

fn request(
    input: &Value,
    discover: impl FnOnce() -> BridgeResult<PathBuf>,
    cancelled: impl Fn() -> bool,
) -> BridgeResult<KeyPair> {
    let (name, curve, path, authentication_ttl) = validate(input)?;
    unlock(
        &Selection {
            helper: discover()?,
            name,
            curve,
            path,
            authentication_ttl,
        },
        cancelled,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WalletSelectionInput {
    name: String,
    #[serde(deserialize_with = "protocol::curve")]
    curve: Curve,
    #[serde(default, deserialize_with = "protocol::present")]
    index: Option<f64>,
    #[serde(default, deserialize_with = "protocol::present")]
    path: Option<String>,
    #[serde(default, rename = "authTTL", deserialize_with = "protocol::present")]
    auth_ttl: Option<f64>,
}

fn validate(input: &Value) -> BridgeResult<(String, Curve, String, u64)> {
    let WalletSelectionInput {
        name,
        curve,
        index,
        path,
        auth_ttl,
    } = protocol::request(input, InvalidWalletSelection)?;
    if !valid_wallet_name(&name) || (path.is_some() && index.is_some()) {
        return Err(InvalidWalletSelection);
    }
    let index = index.unwrap_or_default();
    if !(0.0..2_147_483_648.0).contains(&index) || index.fract() != 0.0 {
        return Err(InvalidWalletSelection);
    }
    let path = match path {
        Some(path) => path,
        None if curve == Curve::Secp256k1 => format!("m/44'/60'/0'/0/{}", index as u32),
        None => return Err(InvalidWalletSelection),
    };
    if !curve.valid_wallet_path(&path) {
        return Err(InvalidWalletSelection);
    }
    let ttl = match auth_ttl {
        Some(value) => protocol::auth_seconds(value).ok_or(InvalidWalletSelection)?,
        None => u64::MAX, // No caller cap: follow the app setting.
    };
    Ok((name, curve, path, ttl))
}

fn unlock(selection: &Selection, cancelled: impl Fn() -> bool) -> BridgeResult<KeyPair> {
    let mut session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| HelperError::Keychain)?;
    if cancelled() {
        return Err(ClientError::Helper(HelperError::Cancelled));
    }
    let reusable = match session.as_mut() {
        Some(helper)
            if helper.path == selection.helper && helper.ttl == selection.authentication_ttl =>
        {
            helper
                .child
                .try_wait()
                .map_err(|_| HelperError::Transport)?
                .is_none()
        }
        _ => false,
    };
    if !reusable {
        *session = None;
        *session = Some(AppSession::start(selection)?);
    }
    let result = session.as_mut().ok_or(HelperError::Transport)?.read_key(
        selection,
        Duration::from_secs(120),
        cancelled,
    );
    if result.is_err() {
        *session = None;
    }
    result
}

/// A helper command with a minimal environment; secrets never come from the environment.
fn helper_command(helper: &Path) -> Command {
    let mut command = Command::new(helper);
    command.env_clear().env("PATH", "/usr/bin:/bin");
    for name in ["HOME", "USER", "LOGNAME", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
}

fn nonblocking(fd: RawFd) -> BridgeResult<()> {
    // SAFETY: `fd` belongs to a live pipe or socket owned by the caller.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    // SAFETY: as above; only the status flags change.
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(HelperError::Transport.into());
    }
    Ok(())
}

fn drain(reader: &mut impl Read, output: &mut Vec<u8>, limit: usize) -> BridgeResult<bool> {
    let mut chunk = Zeroizing::new([0u8; 1024]);
    loop {
        match reader.read(&mut *chunk) {
            Ok(0) => return Ok(true),
            Ok(count) if output.len() + count <= limit => output.extend_from_slice(&chunk[..count]),
            Ok(_) => return Err(InvalidHelperResponse),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(_) => return Err(HelperError::Transport.into()),
        }
    }
}

struct AppSession {
    path: PathBuf,
    ttl: u64,
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: ChildStderr,
    secret: UnixStream,
}

impl Drop for AppSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl AppSession {
    fn start(selection: &Selection) -> BridgeResult<Self> {
        let (secret, child_secret) = UnixStream::pair().map_err(|_| HelperError::Transport)?;
        let fd = child_secret.as_raw_fd();
        let mut command = helper_command(&selection.helper);
        command
            .arg("--bridge")
            .arg(selection.authentication_ttl.to_string())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // SAFETY: the closure runs in the forked child before exec and only calls the
        // async-signal-safe dup2 and fcntl on descriptors that exist in the child.
        unsafe {
            command.pre_exec(move || {
                if libc::dup2(fd, 3) < 0 || libc::fcntl(3, libc::F_SETFD, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = command
            .spawn()
            .map_err(|_| ClientError::HelperStartFailed)?;
        drop(child_secret);
        let streams = (|| {
            let stdin = child.stdin.take().ok_or(HelperError::Transport)?;
            let stdout = child.stdout.take().ok_or(HelperError::Transport)?;
            let stderr = child.stderr.take().ok_or(HelperError::Transport)?;
            for fd in [secret.as_raw_fd(), stdout.as_raw_fd(), stderr.as_raw_fd()] {
                nonblocking(fd)?;
            }
            Ok((stdin, stdout, stderr))
        })();
        let (stdin, stdout, stderr) = match streams {
            Ok(streams) => streams,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        Ok(Self {
            path: selection.helper.clone(),
            ttl: selection.authentication_ttl,
            child,
            stdin,
            stdout,
            stderr,
            secret,
        })
    }

    fn read_key(
        &mut self,
        selection: &Selection,
        timeout: Duration,
        cancelled: impl Fn() -> bool,
    ) -> BridgeResult<KeyPair> {
        let request = json!({"name": selection.name, "curve": selection.curve.name(), "path": selection.path});
        writeln!(self.stdin, "{request}").map_err(|_| HelperError::Transport)?;
        let mut key = Zeroizing::new(Vec::with_capacity(32));
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let deadline = Instant::now() + timeout;
        loop {
            if cancelled() {
                return Err(ClientError::Helper(HelperError::Cancelled));
            }
            let key_done = drain(&mut self.secret, &mut key, 32)?;
            let out_done = drain(&mut self.stdout, &mut out, 2048)?;
            let err_done = drain(&mut self.stderr, &mut err, 1024)?;
            if key.len() == 32 && out.last() == Some(&b'\n') {
                return parse_response(selection, &key, &out, &err);
            }
            if let Some(status) = self.child.try_wait().map_err(|_| HelperError::Transport)?
                && key_done
                && out_done
                && err_done
            {
                return Err(if status.success() {
                    InvalidHelperResponse
                } else {
                    reported_error(&err)
                });
            }
            if Instant::now() >= deadline {
                return Err(ClientError::AuthenticationTimeout);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyPairReply {
    name: String,
    #[serde(deserialize_with = "protocol::curve")]
    curve: Curve,
    path: String,
    public_key: String,
}

fn parse_response(
    selection: &Selection,
    key: &[u8],
    out: &[u8],
    err: &[u8],
) -> BridgeResult<KeyPair> {
    let response: KeyPairReply = protocol::reply(out, err)?;
    if response.name != selection.name
        || response.curve != selection.curve
        || response.path != selection.path
    {
        return Err(InvalidHelperResponse);
    }
    let private: Zeroizing<[u8; 32]> =
        Zeroizing::new(key.try_into().map_err(|_| InvalidHelperResponse)?);
    let public = match selection.curve {
        Curve::Secp256k1 => {
            let signing = k256::ecdsa::SigningKey::from_slice(&*private)
                .map_err(|_| InvalidHelperResponse)?;
            signing
                .verifying_key()
                .to_sec1_point(true)
                .as_bytes()
                .to_vec()
        }
        Curve::Ed25519 => solana_ed25519::ed_sigs::SigningKey::from_bytes(&private)
            .verification_key()
            .as_ref()
            .to_vec(),
    };
    let public = encode_hex(&public);
    if response.public_key != public {
        return Err(InvalidHelperResponse);
    }
    Ok(KeyPair { private, public })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicUsize, Ordering},
    };

    static COUNT: AtomicUsize = AtomicUsize::new(0);
    pub(super) fn folder() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "keylet-bridge-{}-{}",
            std::process::id(),
            COUNT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn curves_defaults_and_index_boundaries() {
        for index in [0, 1, 2, 2_147_483_647] {
            let (_, curve, path, _) =
                validate(&json!({"name":"test", "curve":"secp256k1", "index": index})).unwrap();
            assert_eq!(curve, Curve::Secp256k1);
            assert_eq!(path, format!("m/44'/60'/0'/0/{index}"));
        }
        assert_eq!(
            validate(&json!({"name":"test", "curve":"secp256k1"}))
                .unwrap()
                .2,
            "m/44'/60'/0'/0/0"
        );
        assert!(
            validate(&json!({"name":"test", "curve":"ed25519", "path":"m/2147483647'"})).is_ok()
        );
        assert!(
            validate(&json!({"name":"test", "curve":"secp256k1", "path":"m/44'/60'/1'/1/5"}))
                .is_ok()
        );
    }

    #[test]
    fn invalid_selection_never_discovers_or_launches_a_helper() {
        for input in [
            json!({"name":"test"}),
            json!({"name":"test", "curve":"p256"}),
            json!({"name":"test", "curve":{"secp256k1":null}}),
            json!({"name":"test", "curve":"ed25519"}),
            json!({"name":"test", "curve":"ed25519", "path":"m/0'/1"}),
            json!({"name":"test", "curve":"ed25519", "path":"m/0'", "index":0}),
            json!({"name":"test", "curve":"ed25519", "path":"m/2147483648'"}),
            json!({"name":"test", "curve":"ed25519", "path":"m/0'\n"}),
            json!({"name":"bad\n", "curve":"secp256k1"}),
            json!({"name":"test", "curve":"secp256k1", "helperPath":"relative"}),
            json!({"name":"test", "curve":"secp256k1", "helperPath":"/tmp/helper"}),
            json!({"name":"test", "curve":"secp256k1", "unknown":true}),
            json!({"name":"test", "curve":"secp256k1", "path":null}),
            json!(["test", "secp256k1"]),
        ] {
            assert!(matches!(validate(&input), Err(InvalidWalletSelection)));
        }
        for index in [
            json!(-1),
            json!(0.5),
            json!(2_147_483_648_u64),
            json!("1"),
            Value::Null,
        ] {
            assert!(matches!(
                validate(&json!({"name":"test", "curve":"secp256k1", "index":index})),
                Err(InvalidWalletSelection)
            ));
        }
    }

    #[test]
    fn authentication_duration_follows_app_unless_caller_supplies_a_safe_integer() {
        let mut input = json!({"name":"test", "curve":"secp256k1"});
        assert_eq!(validate(&input).unwrap().3, u64::MAX);
        for seconds in [0, 300, 5400, 86400, 9_007_199_254_740_991u64] {
            input["authTTL"] = json!(seconds);
            assert_eq!(validate(&input).unwrap().3, seconds);
        }
        for invalid in [json!(-1), json!(0.5), json!(9_007_199_254_740_992u64)] {
            input["authTTL"] = invalid;
            assert!(validate(&input).is_err());
        }
    }

    #[test]
    fn keypair_response_rejects_metadata_and_key_mismatches() {
        // RFC 8032 public test vector, never use for funds.
        let key = wallet_crypto::decode_hex(
            "0x9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60",
        )
        .unwrap();
        let selection = Selection {
            helper: PathBuf::new(),
            name: "test".into(),
            curve: Curve::Ed25519,
            path: "m/0'".into(),
            authentication_ttl: 300,
        };
        let mut response = json!({"name":"test", "curve":"ed25519", "path":"m/0'", "publicKey":"0xd75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"});
        let pair = parse_response(&selection, &key, response.to_string().as_bytes(), b"").unwrap();
        assert_eq!(pair.private.as_slice(), key);
        assert_eq!(pair.public, response["publicKey"].as_str().unwrap());
        assert!(
            parse_response(&selection, &key[..31], response.to_string().as_bytes(), b"").is_err()
        );
        response["curve"] = json!("secp256k1");
        assert!(parse_response(&selection, &key, response.to_string().as_bytes(), b"").is_err());
        response["curve"] = json!("ed25519");
        response["publicKey"] = json!(encode_hex(&[0; 32]));
        assert!(parse_response(&selection, &key, response.to_string().as_bytes(), b"").is_err());
        assert_eq!(
            reported_error(b"INTERNAL_ERROR\n"),
            ClientError::Helper(HelperError::Internal)
        );
        assert_eq!(
            reported_error(b"NAME_LIST_CHANGED\n").to_string(),
            HelperError::CatalogChanged.to_string()
        );
        assert_eq!(
            reported_error(b"untrusted exception and pretend secret"),
            ClientError::HelperFailed
        );
    }

    #[test]
    fn stalled_helper_and_inherited_pipes_do_not_outlive_timeout() {
        let root = folder();
        let helper = root.join("helper.sh");
        fs::write(&helper, "#!/bin/sh\n/bin/sleep 2\n").unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let selection = Selection {
            helper,
            name: "test".into(),
            curve: Curve::Ed25519,
            path: "m/0'".into(),
            authentication_ttl: 300,
        };
        let start = Instant::now();
        let result = AppSession::start(&selection).unwrap().read_key(
            &selection,
            Duration::from_millis(40),
            || false,
        );
        // Report the actual code if this ever fails (seen once on CI).
        assert_eq!(result.err(), Some(ClientError::AuthenticationTimeout));
        assert!(start.elapsed() < Duration::from_secs(1));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_unlock_can_be_retried_explicitly() {
        let root = folder();
        let helper = root.join("helper.sh");
        fs::write(
            &helper,
            "#!/bin/sh\nread request\nprintf 'CANCELLED\\n' >&2\nexit 1\n",
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
        let selection = Selection {
            helper: helper.clone(),
            name: "test".into(),
            curve: Curve::Ed25519,
            path: "m/0'".into(),
            authentication_ttl: 300,
        };
        assert_eq!(
            unlock(&selection, || false).err(),
            Some(HelperError::Cancelled.into())
        );
        fs::write(
            &helper,
            "#!/bin/sh\nread request\nprintf 'NOT_FOUND\\n' >&2\nexit 1\n",
        )
        .unwrap();
        assert_eq!(
            unlock(&selection, || false).err(),
            Some(HelperError::Missing.into())
        );
        fs::remove_dir_all(root).unwrap();
    }
}
