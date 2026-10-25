# Keylet Node.js SDK

**English** | [中文](README_ZH.md) · [Keylet overview](../README.md) · [Build and signing](../docs/signing.md)

`keylet` lets Node.js scripts request a derived key pair from the signed macOS app `Keylet.app`. The app authenticates with Touch ID or your Mac password before accessing the recovery phrase in Keychain. Supports secp256k1 and SLIP-0010 Ed25519.

Requires macOS 14+, Node.js 22+, and a separately built, correctly signed app. Returned private keys are controlled by the calling process; never log or persist them.

## Install from source

**Not released yet.** Build from the source checkout instead of installing from the npm registry. First [build and sign Keylet](../README.md#try-it-from-source), then run from the repository root:

```sh
pnpm --dir node install --frozen-lockfile --ignore-scripts
pnpm --dir node run build
```

The build produces `dist/index.js`, `dist/index.d.ts`, and `dist/keylet.node` inside `node/`, targeting the current Mac architecture. To use it in another Node project, run `npm install /absolute/path/keylet/node` there. The `keylet` package does not include `Keylet.app`; install the complete signed app in `~/Applications` or `/Applications`. In the source checkout, the SDK can use `build/Keylet.app` directly.

## Usage

Create or import a wallet in Keylet first. Replace `trading` below with its saved name; the SDK does not create wallets.

```ts
import { getKeyPair } from 'keylet';

const { publicKey, privateKey } = await getKeyPair({
  name: 'trading',
  curve: 'secp256k1',
  index: 0,
});
```

`curve` is required. secp256k1 accepts an optional `index` (default 0) or an explicit EVM `path`, but not both. Ed25519 requires an explicit hardened-only path, such as `{ name: 'trading', curve: 'ed25519', path: "m/44'/501'/0'/0'" }`, and does not accept `index`. Use the original wallet's exact derivation path.

Both result fields are `0x` hex strings. secp256k1 returns a 32-byte private scalar and 33-byte compressed public key; Ed25519 returns a 32-byte child seed and 32-byte public key. Never log or persist private keys. Your application manages its own signing library.

The bridge finds `Keylet.app` in `~/Applications` or `/Applications`. In the source checkout, it can also find `build/Keylet.app`. Otherwise pass the absolute path to `Keylet.app/Contents/MacOS/keylet` as `helperPath`. The app bundle is not included in this npm package; build the app and addon together. Use ESM, or `await import('keylet')` from CommonJS.

Calls reuse a macOS authentication context in a private helper belonging to the current Node process. The fixed window starts after successful authentication and defaults to 300 seconds; repeated calls do not extend it. Keys and failures are not cached. Each call reads Keychain again. Calls must originate from the main thread; different Node processes and App UI operations authenticate separately.

In **… → Settings → Node authentication**, enter a non-negative whole number of minutes and save. The default is 5; `0` authenticates every time. There is no policy maximum; values must fit in unsigned 64-bit seconds. Changes apply to the next Node request, including existing sessions. App recovery-phrase reveals and deletions always require fresh authentication.

`authenticationTtl` optionally requests a shorter window in seconds (a non-negative JavaScript safe integer). Omit it to follow the app setting. `0` requires authentication on every call. The app setting is an upper bound: a caller cannot override it. Changing this parameter starts a fresh helper session.

```ts
const keys = await getKeyPair({
  name: 'trading',
  curve: 'secp256k1',
  authenticationTtl: 0,
});
```

Helper state disappears when the Node process exits. Failures terminate the helper; a subsequent explicit call can retry. Deleting a wallet or locking the screen does not revoke keys already returned to your code.

## Run the examples

From the repository's `node/` directory, using a wallet already saved as `trading`:

```sh
node --import tsx examples/ethers-v5.ts trading 2
node --import tsx examples/ethers-v6.ts trading 2
node --import tsx examples/viem.ts trading 2
node --import tsx examples/ed25519.ts trading "m/44'/501'/2'/0'"
```

The first three examples print only the Ethereum address; the Ed25519 example prints only the hex public key. `index: 2` selects `m/44'/60'/0'/0/2`. Callers choose the Ed25519 path. Do not pass an Ed25519 seed to ethers `Wallet` to derive the corresponding Solana account.

Node.js 22+ is the supported target. Bun's Node-API support may run these examples, but this project has not completed Bun compatibility testing; a successful individual call is not a full compatibility guarantee.

## Troubleshooting

| Error | What to check |
| --- | --- |
| `NOT_FOUND` | The wallet name must match the app; examples never create `trading` or `demo` |
| `KEYLET_APP_NOT_FOUND` | Install the complete app or set `helperPath`; npm does not include the app |
| `KEYLET_BRIDGE_NOT_BUILT` | Check addon build and CPU architecture; `cause` contains the original load error |
| `SIGNING_CONFIGURATION` | Check certificate, profile, bundle ID, and private access group using the signing guide |
| `SETTINGS_ERROR` | Open the app Settings and save an authentication option again |
| `Invalid wallet selection` | curve is required; path and index are exclusive; Ed25519 requires a hardened-only path |
| `CANCELLED` / `AUTHENTICATION_FAILED` | Complete local authentication; an explicit subsequent call can retry |

There is no automatic retry after a failed request. Fix the cause before calling again. Never attach private keys, recovery phrases, or passphrases to an error report.

## Development and publishing

From the repository root:

```sh
pnpm --dir node install --frozen-lockfile --ignore-scripts
pnpm --dir node run build
pnpm --dir node run check
pnpm --dir node test
```

`build` targets the current Mac architecture. The build script lives at `scripts/build-bridge.sh` in the repository root. pnpm manages development dependencies; npm publishes the package. Consumers can use their own package manager.

Before publishing, install both Rust targets and pack from `node/`:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cd node
npm pack
```

`prepack` rebuilds both architectures, merges and checks the native addon, then generates JavaScript and type declarations. A failure stops packaging. The package includes only these three artifacts, documentation, and package metadata; no Rust sources, build scripts, or signing material.

Install the generated `keylet-0.1.0.tgz` in a separate project and verify loading and the public API. After confirming that your publishing account owns the package name, publish that same inspected file with `npm publish ./keylet-0.1.0.tgz`. Update the version in `package.json` before each release. Build the signed app separately from the repository root.

### Manual GitHub Actions release

Use [`.github/workflows/release-node.yml`](../.github/workflows/release-node.yml), **Actions → Release Node.js → Run workflow**:

1. Commit the release changes and push an existing `vMAJOR.MINOR.PATCH` tag matching `node/package.json`. The workflow must be on the default branch to appear in Actions.
2. Enter that tag. Leave `draft` enabled to build and download the `.tgz` without publishing. This mode does not need npm or Apple credentials and does not create a GitHub Release.
3. For npm publication, add the repository Secret `NPM_TOKEN`: a granular access token with publish permission for `keylet` and bypass 2FA enabled for CI, as described in [npm's CI guidance](https://docs.npmjs.com/using-private-packages-in-a-ci-cd-workflow/). Confirm package-name ownership and that the package's publishing policy permits token-based publication.
4. Run again with the same tag and `draft` disabled. This rebuilds and verifies the tarball, uploads it to workflow artifacts, then publishes that same verified file to npm with the `latest` dist-tag. Existing npm versions cannot be overwritten; increment the version and create a new Git tag for the next release.

CI reuses `prepack` to build the Universal addon and TypeScript outputs, checks the exact package file list, installs the tarball outside the checkout, and checks addon loading and rejection of invalid input without accessing a wallet. It checks both Mach-O architectures, but executes the smoke check only on the runner's architecture; it does not replace Intel runtime or interactive app validation. No npm token is passed to build or package scripts. The App and npm package use separate workflows (`release-app.yml` and `release-node.yml`); each tag must match the corresponding package version.

## License

[MIT](LICENSE)
