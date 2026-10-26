# Testing

Run from the repository root:

```sh
pnpm --dir node install --frozen-lockfile --ignore-scripts
bash scripts/check-native.sh
pnpm --dir node run build
pnpm --dir node run check
pnpm --dir node test
```

Rust tests cover derivation vectors, backup confirmation, protected Keychain queries, secure input, clipboard restrictions, the public catalog, deletion plans, and helper transport. Python tests validate packaging with synthetic public profile metadata and exercise universal-addon packaging with stub build tools. Node tests load the native addon and use public fixtures for both curves, key correspondence, authentication serialization, process isolation, and failure handling.

Ed25519 tests check the Solana backend against official SLIP-0010 vectors and public mnemonic results captured before the backend replacement, including account paths, a passphrase, and the maximum hardened index. They also check secret and chain-code zeroization.

## Interactive checks

Use an unfunded test wallet and complete authentication locally. Check creation, backup confirmation, import, key reads, cancellation, Touch ID or password fallback, and profile renewal. Do not send recovery phrases or private keys to chat. Automated tests do not delete real wallets or execute blockchain transactions.

## npm release artifact

Follow [the Node release steps](../node/README.md#development-and-publishing) to build a universal tarball. Install the tarball in a clean Node project; do not use a source-directory symlink for this check. Confirm that importing the package works and an invalid wallet selection is rejected before helper discovery. Check the tarball file list for only package metadata, READMEs, `dist/index.js`, `dist/index.d.ts`, and `dist/keylet.node`. Run the public-fixture integration tests on both Apple Silicon and Intel before claiming support is verified on both.

Interactive clipboard regression: copy a test phrase, return to the wallet list, then open Sync from Keychain and leave authentication pending. The app's clipboard copy must already be cleared. New unrelated clipboard contents must remain untouched.

Clipboard shortcuts: verify Cmd+C, Cmd+X, and Cmd+V in wallet search, wallet details setup, and settings. Enter the secrets and backup-confirmation pages and verify those shortcuts remain blocked, including in sidebar search while a sensitive page is open. Return to a public page and verify shortcuts work again. The explicit seed-phrase Copy button must still work and retain its 60-second cleanup.

Backup confirmation uses 12 masked fields with shuffled numeric labels. Enter each word at its original number; Save must reject incomplete, misplaced, or multiword entries. Skip explicitly saves without verifying the backup; Cancel must not save anything. The order stays fixed while editing and is shuffled again when re-entering confirmation. Confirm that Tab moves through the displayed fields, returning to backup clears the entries, and Cmd+V cannot fill them. A skipped backup remains available through Seed phrase only after fresh macOS authentication.

Node authentication regression: use an unfunded test wallet and the same Node process. With the app duration saved as **5 minutes**, call twice with `authenticationTtl: 1`: only the first call should prompt. Wait over one second and call again: it should prompt again. With `authenticationTtl: 0`, every call must prompt. In a running session, save the app duration as **0 minutes** and verify the next call prompts even with `authenticationTtl: 300`. Cancel authentication, then explicitly retry and verify it can prompt again. Different Node processes must authenticate independently. App reveals and deletions must still use fresh authentication; do not delete a real wallet for this check.

The SDK and signed app must be rebuilt together after changes to the private helper protocol. Automated fixtures verify transport and timing logic; they do not establish real macOS prompt reuse or password fallback behavior.
