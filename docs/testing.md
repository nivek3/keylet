# Testing

Run from the repository root:

```sh
pnpm --dir node install --frozen-lockfile --ignore-scripts
bash scripts/check-native.sh
pnpm --dir node run build
pnpm --dir node run check
pnpm --dir node test
```

Rust tests cover derivation vectors, backup confirmation, protected Keychain queries, secure input, clipboard restrictions, the public catalog, deletion plans, and helper transport. Python tests validate packaging with synthetic public profile metadata and exercise universal-addon packaging with stub build tools. `pnpm --dir node test` first builds a separate debug addon for public-fixture transport tests covering both curves, key correspondence, authentication serialization, process isolation, and failure handling. Its fixture-only entry point is excluded from release builds and the npm package; public validation and release-artifact tests use the release addon.

Ed25519 tests check the Solana backend against official SLIP-0010 vectors and public mnemonic results captured before the backend replacement, including account paths, a passphrase, and the maximum hardened index. They also check secret and chain-code zeroization.

App discovery tests use synthetic bundles to cover renamed apps, executable metadata, and mismatched Bundle IDs. Invalid-input tests reject removed path overrides and unknown fields before discovery. These tests do not register or launch real apps.

## Interactive checks

Use an unfunded test wallet and complete authentication locally. Check creation, backup confirmation, import, key reads, cancellation, Touch ID or password fallback, and profile renewal. Do not send recovery phrases or private keys to chat. Automated tests do not delete real wallets or execute blockchain transactions.

Node SDK discovery: install and open a signed app with Bundle ID `com.nivek3.keylet`, then call the SDK. Rename or move the app, open it again so macOS registers the new location, and repeat. With multiple registered copies, confirm the SDK uses the system's preferred copy. Verify missing or invalid selected apps fail without filename-based fallback. Existing development apps with another ID are outside the SDK's lookup target; keep their signing/Keychain identity instead of changing it just to run this check.

Derived accounts: expand **Derived account** in wallet details. Check Ethereum indices 0 and 2 against the SDK results, and Solana indices against the full paths `m/44'/501'/0'/0'` and `m/44'/501'/2'/0'`. Verify a custom full path, reject negative/overflowing indices and non-hardened Solana paths, and confirm the displayed path matches the query. Each query must require fresh authentication; cancellation must leave no address result. Copy must contain only the displayed address. Changing the curve, index, input mode, or path, and switching wallets (including through search), must clear the previous result. Check the form and wrapped address in the minimum window size.

## npm release artifact

Follow [the Node release steps](../node/README.md#development-and-publishing) to build a universal tarball. Install the tarball in a clean Node project; do not use a source-directory symlink for this check. Confirm that importing the package works and an invalid wallet selection is rejected before helper discovery. Check the tarball file list for only package metadata, READMEs, `dist/index.js`, `dist/index.d.ts`, and `dist/keylet.node`. Run the public-fixture integration tests on both Apple Silicon and Intel before claiming support is verified on both.

Interactive clipboard regression: copy a test phrase, return to the wallet list, then open Sync from Keychain and leave authentication pending. The app's clipboard copy must already be cleared. New unrelated clipboard contents must remain untouched.

Clipboard shortcuts: verify Cmd+C, Cmd+X, and Cmd+V in wallet search, wallet details setup, and settings. Enter the secrets and backup-confirmation pages and verify those shortcuts remain blocked, including in sidebar search while a sensitive page is open. Return to a public page and verify shortcuts work again. The explicit seed-phrase Copy button must still work and retain its 60-second cleanup.

Backup confirmation uses 12 masked fields with shuffled numeric labels. Enter each word at its original number; Save must reject incomplete, misplaced, or multiword entries. Skip explicitly saves without verifying the backup; Cancel must not save anything. The order stays fixed while editing and is shuffled again when re-entering confirmation. Confirm that Tab moves through the displayed fields, returning to backup clears the entries, and Cmd+V cannot fill them. A skipped backup remains available through Seed phrase only after fresh macOS authentication.

Node authentication regression: use an unfunded test wallet and the same Node process. With the app duration saved as **5 minutes**, call twice with `authTTL: 1`: only the first call should prompt. Wait over one second and call again: it should prompt again. With `authTTL: 0`, every call must prompt. In a running session, save the app duration as **0 minutes** and verify the next call prompts even with `authTTL: 300`. Cancel authentication, then explicitly retry and verify it can prompt again. Different Node processes must authenticate independently. App reveals and deletions must still use fresh authentication; do not delete a real wallet for this check.

The SDK and signed app must be rebuilt together after changes to the private helper protocol. Automated fixtures verify transport and timing logic; they do not establish real macOS prompt reuse or password fallback behavior.
