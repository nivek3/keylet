//! CPython binding for the Keylet macOS client.

use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::{PyInt, PyString},
};
use serde_json::{Value, json};
use std::{
    process,
    sync::atomic::{AtomicU32, Ordering},
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

fn read_key_pair(
    py: Python<'_>,
    request: impl FnOnce() -> Result<wallet_client::KeyPair, &'static str> + Send,
) -> PyResult<KeyPair> {
    let pid = process::id();
    // A fork can inherit the parent's helper pipes or a locked mutex.
    if let Err(previous) = CALLER_PID.compare_exchange(0, pid, Ordering::Relaxed, Ordering::Relaxed)
        && previous != pid
    {
        return Err(PyRuntimeError::new_err("FORKED_PROCESS"));
    }
    let pair = py.detach(request).map_err(|code| {
        if code == wallet_client::INVALID {
            PyValueError::new_err(code)
        } else {
            PyRuntimeError::new_err(code)
        }
    })?;
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
    read_key_pair(py, move || wallet_client::key_pair(&input))
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
    read_key_pair(py, move || wallet_client::key_pair_for_test(&input, executable))
}

#[pymodule]
fn keylet(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<KeyPair>()?;
    module.add_function(wrap_pyfunction!(get_key_pair, module)?)?;
    #[cfg(debug_assertions)]
    module.add_function(wrap_pyfunction!(_get_key_pair_for_test, module)?)?;
    Ok(())
}
