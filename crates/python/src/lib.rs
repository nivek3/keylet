//! `CPython` binding for the Keylet macOS client.

use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::{PyInt, PyString},
};
use serde_json::{Value, json};
use std::{
    fmt::Write as _,
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::Duration,
};
use zeroize::Zeroizing;

static CALLER_PID: AtomicU32 = AtomicU32::new(0);

/// A derived public and private key pair in lowercase `0x` hex.
#[pyclass(frozen, module = "keylet")]
struct KeyPair {
    #[pyo3(get)]
    public_key: Py<PyString>,
    #[pyo3(get)]
    private_key: Py<PyString>,
}

fn integer(value: &Bound<'_, PyAny>) -> PyResult<i64> {
    value
        .cast_exact::<PyInt>()
        .ok()
        .and_then(|value| value.extract().ok())
        .ok_or_else(|| PyValueError::new_err(wallet_client::INVALID))
}

fn selection(
    name: &str,
    curve: &str,
    index: Option<&Bound<'_, PyAny>>,
    path: Option<&str>,
    auth_ttl: Option<&Bound<'_, PyAny>>,
) -> PyResult<Value> {
    let mut input = json!({ "name": name, "curve": curve });
    if let Some(index) = index {
        input["index"] = json!(integer(index)?);
    }
    if let Some(path) = path {
        input["path"] = json!(path);
    }
    if let Some(auth_ttl) = auth_ttl {
        input["authTTL"] = json!(integer(auth_ttl)?);
    }
    Ok(input)
}

/// Runs a blocking client request off the GIL while still handling Python signals.
fn run<T: Send + 'static>(
    py: Python<'_>,
    request: impl FnOnce(&AtomicBool) -> Result<T, &'static str> + Send + 'static,
) -> PyResult<T> {
    let pid = process::id();
    // A fork can inherit the parent's helper pipes or a locked mutex.
    if let Err(previous) = CALLER_PID.compare_exchange(0, pid, Ordering::Relaxed, Ordering::Relaxed)
        && previous != pid
    {
        return Err(PyRuntimeError::new_err("FORKED_PROCESS"));
    }
    // Python signal handlers can run arbitrary code, so check them outside the client lock.
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    let (send, mut receive) = mpsc::channel();
    thread::Builder::new()
        .spawn(move || {
            let _ = send.send(request(&worker_cancelled));
        })
        .map_err(|_| PyRuntimeError::new_err("THREAD_START_FAILED"))?;

    let result = loop {
        let receiver = &mut receive;
        let result = py.detach(move || receiver.recv_timeout(Duration::from_millis(25)));
        if let Err(error) = py.check_signals() {
            cancelled.store(true, Ordering::Relaxed);
            return Err(error);
        }
        match result {
            Ok(result) => break result,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(PyRuntimeError::new_err("CLIENT_REQUEST_FAILED"));
            }
        }
    };
    result.map_err(|code| {
        if code == wallet_client::INVALID || code == wallet_client::signer::INVALID_SIGNER {
            PyValueError::new_err(code)
        } else {
            PyRuntimeError::new_err(code)
        }
    })
}

fn read_key_pair(
    py: Python<'_>,
    request: impl FnOnce(&AtomicBool) -> Result<wallet_client::KeyPair, &'static str> + Send + 'static,
) -> PyResult<KeyPair> {
    let pair = run(py, request)?;
    let (public_key, private_key) = pair.into_hex();
    let private_key = Zeroizing::new(private_key);
    Ok(KeyPair {
        public_key: PyString::new(py, &public_key).unbind(),
        private_key: PyString::new(py, private_key.as_str()).unbind(),
    })
}

/// Request a derived key pair from the registered Keylet app.
#[pyfunction]
#[pyo3(signature = (*, name, curve, index=None, path=None, auth_ttl=None))]
fn get_key_pair(
    py: Python<'_>,
    name: &str,
    curve: &str,
    index: Option<&Bound<'_, PyAny>>,
    path: Option<&str>,
    auth_ttl: Option<&Bound<'_, PyAny>>,
) -> PyResult<KeyPair> {
    let input = selection(name, curve, index, path, auth_ttl)?;
    read_key_pair(py, move |cancelled| {
        wallet_client::key_pair(&input, || cancelled.load(Ordering::Relaxed))
    })
}

#[cfg(debug_assertions)]
#[pyfunction]
#[pyo3(signature = (executable, *, name, curve, index=None, path=None, auth_ttl=None))]
fn _get_key_pair_for_test(
    py: Python<'_>,
    executable: &str,
    name: &str,
    curve: &str,
    index: Option<&Bound<'_, PyAny>>,
    path: Option<&str>,
    auth_ttl: Option<&Bound<'_, PyAny>>,
) -> PyResult<KeyPair> {
    let input = selection(name, curve, index, path, auth_ttl)?;
    let executable = executable.into();
    read_key_pair(py, move |cancelled| {
        wallet_client::key_pair_for_test(&input, executable, || cancelled.load(Ordering::Relaxed))
    })
}

/// A signature from a local signer, verified against its public key. Fields are `0x` hex.
#[pyclass(frozen, module = "keylet", get_all)]
#[expect(
    clippy::struct_field_names,
    reason = "field names are the Python attributes, matching the Node SDK"
)]
struct Signature {
    name: String,
    curve: String,
    /// secp256k1: 33-byte compressed key. Ed25519: 32-byte public key.
    public_key: String,
    /// 64 bytes. secp256k1: `r || s`, low-S. Ed25519: the standard signature.
    signature: String,
    /// secp256k1 only: 0 or 1.
    recovery_id: Option<u8>,
    /// Ed25519 only: the Base58 public key (Solana address).
    address: Option<String>,
}

#[pymethods]
impl Signature {
    fn __repr__(&self) -> String {
        format!(
            "Signature(name={:?}, curve={:?}, public_key={:?})",
            self.name, self.curve, self.public_key
        )
    }
}

/// A local signer's public key.
#[pyclass(frozen, module = "keylet", get_all)]
struct SignerPublicKey {
    name: String,
    curve: String,
    public_key: String,
    address: Option<String>,
}

#[pymethods]
impl SignerPublicKey {
    fn __repr__(&self) -> String {
        format!(
            "SignerPublicKey(name={:?}, curve={:?}, public_key={:?})",
            self.name, self.curve, self.public_key
        )
    }
}

fn sign_input(name: &str, payload: &[u8], auth_ttl: Option<&Bound<'_, PyAny>>) -> PyResult<Value> {
    let hex = payload.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    let mut input = json!({ "name": name, "payload": format!("0x{hex}") });
    if let Some(auth_ttl) = auth_ttl {
        input["authTTL"] = json!(
            integer(auth_ttl)
                .map_err(|_| PyValueError::new_err(wallet_client::signer::INVALID_SIGNER))?
        );
    }
    Ok(input)
}

fn signature(value: wallet_client::signer::Signature) -> Signature {
    Signature {
        name: value.name,
        curve: value.curve.name().to_owned(),
        public_key: value.public_key,
        signature: value.signature,
        recovery_id: value.recovery_id,
        address: value.address,
    }
}

fn signer_public_key(value: wallet_client::signer::SignerPublicKey) -> SignerPublicKey {
    SignerPublicKey {
        name: value.name,
        curve: value.curve.name().to_owned(),
        public_key: value.public_key,
        address: value.address,
    }
}

/// Sign `payload` (bytes) with a local signer in the Keylet app.
///
/// Ed25519 signs 1 to 1232 raw bytes; secp256k1 signs exactly one 32-byte digest.
/// Keylet signs blindly. Calls with the same name and `auth_ttl` share one session.
#[pyfunction]
#[pyo3(signature = (*, name, payload, auth_ttl=None))]
fn sign_with_signer(
    py: Python<'_>,
    name: &str,
    payload: &[u8],
    auth_ttl: Option<&Bound<'_, PyAny>>,
) -> PyResult<Signature> {
    let input = sign_input(name, payload, auth_ttl)?;
    run(py, move |cancelled| {
        wallet_client::signer::sign(&input, || cancelled.load(Ordering::Relaxed))
    })
    .map(signature)
}

/// Read a local signer's public key. The app asks for macOS authentication.
#[pyfunction]
#[pyo3(signature = (*, name))]
fn get_signer_public_key(py: Python<'_>, name: &str) -> PyResult<SignerPublicKey> {
    let input = json!({ "name": name });
    run(py, move |cancelled| {
        wallet_client::signer::public_key(&input, || cancelled.load(Ordering::Relaxed))
    })
    .map(signer_public_key)
}

#[cfg(debug_assertions)]
#[pyfunction]
#[pyo3(signature = (executable, *, name, payload, auth_ttl=None))]
fn _sign_with_signer_for_test(
    py: Python<'_>,
    executable: &str,
    name: &str,
    payload: &[u8],
    auth_ttl: Option<&Bound<'_, PyAny>>,
) -> PyResult<Signature> {
    let input = sign_input(name, payload, auth_ttl)?;
    let executable = executable.into();
    run(py, move |cancelled| {
        wallet_client::signer::sign_for_test(&input, executable, || {
            cancelled.load(Ordering::Relaxed)
        })
    })
    .map(signature)
}

#[cfg(debug_assertions)]
#[pyfunction]
#[pyo3(signature = (executable, *, name))]
fn _get_signer_public_key_for_test(
    py: Python<'_>,
    executable: &str,
    name: &str,
) -> PyResult<SignerPublicKey> {
    let input = json!({ "name": name });
    let executable = executable.into();
    run(py, move |cancelled| {
        wallet_client::signer::public_key_for_test(&input, executable, || {
            cancelled.load(Ordering::Relaxed)
        })
    })
    .map(signer_public_key)
}

#[pymodule]
fn keylet(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<KeyPair>()?;
    module.add_class::<Signature>()?;
    module.add_class::<SignerPublicKey>()?;
    module.add_function(wrap_pyfunction!(get_key_pair, module)?)?;
    module.add_function(wrap_pyfunction!(sign_with_signer, module)?)?;
    module.add_function(wrap_pyfunction!(get_signer_public_key, module)?)?;
    #[cfg(debug_assertions)]
    {
        module.add_function(wrap_pyfunction!(_get_key_pair_for_test, module)?)?;
        module.add_function(wrap_pyfunction!(_sign_with_signer_for_test, module)?)?;
        module.add_function(wrap_pyfunction!(_get_signer_public_key_for_test, module)?)?;
    }
    Ok(())
}
