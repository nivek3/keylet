# Keylet Python SDK

Use the Python SDK to request a derived key pair from the signed `Keylet.app`. The app accesses Keychain and uses macOS authentication. Python receives the derived private key in its own process. Keep it out of logs, files, and telemetry.

## Install

Use macOS 14 or later and a standard CPython 3.10 or later build. Install the signed `Keylet.app` separately and open it once to register its Bundle ID, `com.nivek3.keylet`. The app can stay closed during SDK calls. Use a trusted app installation. The SDK uses the registered Bundle ID for discovery. Publisher and child process verification are tracked in [issue #3](https://github.com/nivek3/keylet/issues/3).

Download the wheel for Apple Silicon (`arm64`) or Intel (`x86_64`) from the Python job in [Checks](https://github.com/nivek3/keylet/actions/workflows/check.yml). Extract the artifact, then install the wheel in a virtual environment. This command uses the Apple Silicon wheel:

```sh
python -m pip install ./keylet_macos-0.1.0-cp310-abi3-macosx_14_0_arm64.whl
```

The distribution is named `keylet-macos` and the import is `keylet`. Use a separate environment if you have the unrelated `keylet` distribution from PyPI installed.

To build from source, follow the [app signing instructions](../docs/signing.md). With Rust and the Apple command-line tools available, run this command from the repository root:

```sh
python -m pip install ./python
```

## Get a key pair

```python
from keylet import get_key_pair

keys = get_key_pair(name="trading", curve="secp256k1", index=0)
solana_keys = get_key_pair(
    name="trading",
    curve="ed25519",
    path="m/44'/501'/0'/0'",
    auth_ttl=0,
)
```

- For `secp256k1`, pass an `index` from 0 to 2³¹ − 1 to select `m/44'/60'/0'/0/{index}`, or pass a full `path`. The default index is 0.
- For SLIP-0010 `ed25519`, pass a full hardened `path`. Both SDKs derive the same keys for the same wallet and path.
- Read the result from `keys.public_key` and `keys.private_key`. Both are lowercase `0x` hex. The secp256k1 public key is compressed and the Ed25519 private key is a 32-byte seed.
- The optional `auth_ttl` sets a shorter reuse window in whole seconds, from 0 to 2⁵³ − 1. By default the SDK follows the app's **Settings → Reuse Node auth** value, which also applies to Python. The default app setting and an explicit `auth_ttl=0` both require authentication on each call.

The fixed reuse window begins when authentication succeeds. Calls reuse the context within the same Python process, and each call reads Keychain again. For multiprocessing, start workers with `spawn`. A child forked after the SDK was used raises `RuntimeError("FORKED_PROCESS")`.

Invalid wallet selections raise `ValueError`. Helper failures raise `RuntimeError` with the same message codes as the Node SDK, including `KEYLET_APP_NOT_FOUND` and `CANCELLED`. After a failure, a later call can retry. See [Node error codes](../node/README.md#troubleshooting).

## Sign with a local signer

Local signers keep the private key in the app and return only signatures. Create one with the app's CLI first, for example `keylet signer create evm --curve secp256k1`. See [Local signing](../README.md#local-signing).

```python
from keylet import get_signer_public_key, sign_with_signer

key = get_signer_public_key(name="evm")  # Prompts for macOS authentication.
signed = sign_with_signer(name="evm", payload=sighash, auth_ttl=60)
print(signed.signature, signed.recovery_id)
```

- `payload` is `bytes`. Ed25519 signers sign 1 to 1232 raw bytes, such as a serialized Solana transaction message. secp256k1 signers sign exactly one 32-byte digest, such as an Ethereum transaction hash or a Bitcoin sighash.
- The result has `name`, `curve`, `public_key`, and a 64-byte `signature` in `0x` hex. secp256k1 results add `recovery_id` (0 or 1) and use low-S `r || s`. Ed25519 results add `address`, the Base58 public key. The SDK verifies every signature against the public key before returning it.
- **Keylet signs blindly.** It does not decode payloads or apply transaction policies.
- Calls with the same `name` and `auth_ttl` share one signing session in this process. The app's **Settings → Reuse signer auth** sets how long a session may sign without asking again (default 0). `auth_ttl` can only shorten that window. Invalid requests raise `ValueError("Invalid signer request")`, and app failures raise `RuntimeError` with the same codes as `get_key_pair`.

The [`examples/`](examples) directory signs a Bitcoin sighash for a P2WPKH witness and a Solana message, using only the standard library:

```sh
python examples/sign_bitcoin_sighash.py evm <32-byte sighash hex>
python examples/sign_solana_message.py sol "hello"
```

## Build and test a wheel

Run these commands from the `python/` directory in an activated virtual environment:

```sh
python -m pip install 'maturin>=1.9.4,<2'
maturin build --release
```

Install the resulting wheel in a separate virtual environment and run `tests/test_installed.py` with its Python interpreter. The fixture tests use a development wheel built with `maturin build --profile dev`. Install it in a separate environment and run `tests/test_fixture.py`. The fixture tests use public keys and a simulated helper, so they require no App or wallet. The release wheel omits the fixture entry point.
