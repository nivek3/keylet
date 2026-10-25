# macOS signing

The repository builds the Rust app with Cargo and packages it using `scripts/build-native.sh`. The script requires a code-signing identity in an accessible Keychain and a valid macOS provisioning profile authorizing the bundle identifier and private Keychain group.

## Personal Team

A free Personal Team manages its App IDs, certificates, devices, and profiles through Xcode. Provisioning expires periodically. Xcode is needed for that account workflow, while this repository's app build uses Cargo and command-line signing. See [Apple's Personal Team documentation](https://developer.apple.com/help/account/basics/about-your-developer-account).

To prepare signing assets:

1. Sign in to your Apple Account in Xcode Settings.
2. Create a temporary macOS App project outside this repository. Set the desired bundle ID, select your Personal Team, enable automatic signing, and configure Keychain Sharing with the app's private access group.
3. Build for My Mac. Locate the signed app through Products → Show in Finder. Its `Contents/embedded.provisionprofile` can be used while valid, with the matching Apple Development signing identity.

If signing assets already exist, use them directly. For local development, keep certificate private keys in Keychain. For CI, store the exported signing certificate and its password only in the repository Secrets described below. Never store signing assets, passwords, or wallet secrets in Git.

## Build

From the repository root:

```sh
bash scripts/build-native.sh \
  'Apple Development: Your Name (TEAMID)' \
  '/absolute/path/App.provisionprofile' \
  'com.example.keychain-wallet'
```

The script reads the public profile metadata, validates its expiry and authorization, builds the Rust executable, embeds the profile, signs with Hardened Runtime, and verifies the signature. The profile's App Identifier Prefix determines the private access group; it is not assumed to equal Team ID. The output is `build/Keylet.app`. An existing output is replaced only after the new bundle passes signature verification. Both bundle version fields follow `crates/app/Cargo.toml`; use a numeric `major.minor.patch` version.

The default build targets the host Mac. To build a Universal app for Apple Silicon and Intel, install both Rust targets and append `--universal` to the build command:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bash scripts/build-native.sh \
  'Developer ID Application: Your Name (TEAMID)' \
  '/absolute/path/App.provisionprofile' \
  'com.example.keychain-wallet' --universal
```

```sh
open build/Keylet.app
build/Keylet.app/Contents/MacOS/keylet check-signing
```

`check-signing` validates public signing configuration. It does not establish that real Keychain authentication succeeds. Use an unfunded test wallet in a logged-in graphical session to check authentication and cancellation.

For local Node use, the SDK discovers `build/Keylet.app` from this source checkout. For a separate installation, place the complete app in `~/Applications` or `/Applications`, or specify the executable's absolute path as `helperPath`.

## Renewal

The app is `Keylet.app`, with executable `Contents/MacOS/keylet`. The Keychain service and `Library/Application Support/KeychainWallet` directory keep their existing storage identifiers to preserve local wallets and settings. Reuse the same signing identity, bundle ID, and authorized access group when rebuilding.

Renew Personal Team assets through Xcode, then rerun the script with the current profile and matching identity. Keep the bundle ID and authorized access group stable when retaining saved wallets. Profiles do not back up wallet data; keep an independent offline recovery backup.

Do not disable signing checks, the private Keychain group, Hardened Runtime, or user-presence authentication to bypass errors. Ad hoc signing is insufficient. The script does not notarize or publish the app.

References: [command-line signing](https://developer.apple.com/documentation/xcode/creating-distribution-signed-code-for-the-mac/), [Keychain implementations](https://developer.apple.com/documentation/technotes/tn3137-on-mac-keychains).

## Public distribution

The build script does not notarize the app. The manual CI workflow below handles notarization and publication. A Personal Team development build is not a general-purpose download for other users.

For a downloadable release, the publisher needs Developer ID signing through the Apple Developer Program, the entitlements/profile required by this app, notarization, and a stapled ticket. Publish separately identified Apple Silicon and Intel builds, or a validated universal app. Test Keychain creation, read, cancellation, and deletion on a different Mac before advertising the download. End users of that release do not need Xcode, Rust, or their own developer account.

The npm native addon and the signed app are separate artifacts. A universal addon does not make a host-only app universal. Keep the production bundle ID and Keychain access group stable across updates.

See [Apple Developer ID distribution](https://developer.apple.com/developer-id/) and [notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).

## Manual macOS release CI

[Release macOS App](../.github/workflows/release-app.yml) follows cc-switch's [macOS release flow](https://github.com/farion1231/cc-switch/blob/main/.github/workflows/release.yml): merge both architectures, sign, notarize, staple, and verify the app, then create, sign, notarize, and staple a DMG. This repository uses its existing Cargo build scripts and macOS's built-in `hdiutil`. It ships `Keylet-v<version>-macOS-universal.dmg`, containing `Keylet.app` and an Applications shortcut, plus `SHA256SUMS.txt` calculated after stapling. Both the app and DMG use the same Developer ID Application identity; no additional certificate or Secret is required. See [Apple's packaging guidance](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution).

Only `workflow_dispatch` triggers this workflow; pushing a branch or tag does not publish anything. The default is a draft GitHub Release. No signing credentials are supplied by the repository, and missing configuration stops the job before building.

In **Settings → Secrets and variables → Actions**, configure these repository entries:

| Type | Name | Value |
| --- | --- | --- |
| Variable | `APPLE_BUNDLE_ID` | Production bundle ID authorized by the profile; keep it stable across updates. |
| Variable | `APPLE_SIGNING_IDENTITY` | Full `Developer ID Application: … (…)` identity matching the exported certificate. |
| Secret | `APPLE_CERTIFICATE` | Base64-encoded `.p12` containing the Developer ID Application certificate and its private key. |
| Secret | `APPLE_CERTIFICATE_PASSWORD` | Password protecting that `.p12`. |
| Secret | `APPLE_PROVISIONING_PROFILE` | Base64-encoded Developer ID distribution `.provisionprofile` authorizing the bundle ID and private Keychain group. Do not use a Personal Team development profile. |
| Secret | `APPLE_ID` | Apple account used for notarization. |
| Secret | `APPLE_PASSWORD` | App-specific password for that account, not its login password. |
| Secret | `APPLE_TEAM_ID` | Developer team ID for notarization. |

The job generates its temporary Keychain password and uses GitHub's built-in `GITHUB_TOKEN`; neither needs an additional secret. Signing files and the temporary Keychain are cleaned up even when a step fails. The certificate/profile use the [GitHub-recommended Secrets mechanism](https://docs.github.com/en/actions/how-tos/deploy/deploy-to-third-party-platforms/sign-xcode-applications).

To copy each encoded file directly to the clipboard without printing it in the terminal:

```sh
base64 -i /absolute/path/DeveloperID.p12 | pbcopy
```

Paste it into `APPLE_CERTIFICATE`, then repeat for the profile and paste into `APPLE_PROVISIONING_PROFILE`:

```sh
base64 -i /absolute/path/App.provisionprofile | pbcopy
```

To release:

1. Commit the workflow, scripts, and desired `crates/app/Cargo.toml` version. Keep the workspace lockfile consistent with any package version change. The workflow must be on the default branch to appear in Actions, and the release tag must include the updated build scripts.
2. Create and push a tag such as `v0.1.0` pointing to the commit to build. Only `vMAJOR.MINOR.PATCH` tags are supported; the tag must match `wallet-app`'s package version.
3. Open **Actions → Release macOS App → Run workflow**, enter that existing tag, and keep `draft` enabled. The checkout uses this tag, independently of the branch selected for the workflow.
4. Download `Keylet-v0.1.0-macOS-universal.dmg` from the workflow artifacts or draft Release. Open it and drag `Keylet.app` to Applications. Check it on a separate Mac, then publish the draft in GitHub Releases. Disable `draft` at dispatch only when you intend to publish immediately after successful verification.

The macOS job verifies both binary architectures, the app's public signing configuration, and accepted notarization, stapled tickets, and Gatekeeper assessment for both the app and DMG. It also mounts the final DMG read-only and verifies the Applications shortcut and the embedded app's architectures, signature, ticket, and Gatekeeper assessment. It does not exercise interactive Keychain authentication or run the test suite; run the checks in [testing](testing.md) before selecting the release commit. To release the npm package separately, use `release-node.yml` and follow the [Node release instructions](../node/README.md#manual-github-actions-release).

An existing Release is not overwritten. If creation fails, the completed DMG remains in workflow artifacts. If either notarization times out, Apple may continue processing; inspect the submission before starting another run. Keep the production App Identifier Prefix, bundle ID, and Keychain group stable: switching from a development identity can make existing development wallets inaccessible to the distributed app.
