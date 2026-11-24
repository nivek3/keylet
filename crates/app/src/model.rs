use serde::{Deserialize, Serialize};
pub use wallet_crypto::Curve;
use wallet_crypto::{DeriveError, PublicKey, Seed};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub type Result<T> = std::result::Result<T, Failure>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Cancelled,
    Authentication,
    Missing,
    Duplicate,
    Keychain,
    Configuration,
    Input,
    Mnemonic,
    Derivation,
    PublicKey,
    Address,
    Transport,
    Corrupt,
    Catalog,
    CatalogChanged,
    Settings,
}
impl Failure {
    pub fn code(self) -> &'static str {
        match self {
            Self::Cancelled => "CANCELLED",
            Self::Authentication => "AUTHENTICATION_FAILED",
            Self::Missing => "NOT_FOUND",
            Self::Duplicate => "ALREADY_EXISTS",
            Self::Keychain => "KEYCHAIN_ERROR",
            Self::Configuration => "SIGNING_CONFIGURATION",
            Self::Input => "INVALID_INPUT",
            Self::Mnemonic => "INVALID_MNEMONIC",
            Self::Derivation => "DERIVATION_FAILED",
            Self::PublicKey => "PUBLIC_KEY_MISMATCH",
            Self::Address => "ADDRESS_MISMATCH",
            Self::Transport => "TRANSPORT_ERROR",
            Self::Corrupt => "INVALID_ITEM",
            Self::Catalog => "NAME_LIST_ERROR",
            Self::CatalogChanged => "NAME_LIST_CHANGED",
            Self::Settings => "SETTINGS_ERROR",
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Settings => {
                "Unable to read authentication settings. Open Settings and save an option."
            }
            Self::Mnemonic => "Check the seed phrase words and their order.",
            Self::Address => {
                "The seed phrase, passphrase, or path doesn't match this Ethereum address."
            }
            Self::PublicKey => {
                "The seed phrase, passphrase, or path doesn't match this Ed25519 public key."
            }
            Self::Duplicate => "That name already exists. The existing wallet was not changed.",
            Self::Configuration => {
                "The app's signing or secure-input configuration is invalid. Rebuild with your Personal Team."
            }
            Self::Authentication => "macOS authentication was not completed. Nothing was unlocked.",
            _ => "Check the name, path, and passphrase. Nothing was saved.",
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct KeyRequest {
    pub name: String,
    pub path: String,
    pub curve: Curve,
}
impl KeyRequest {
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name)?;
        if self.curve.valid_wallet_path(&self.path) {
            Ok(())
        } else {
            Err(Failure::Input)
        }
    }
    pub fn validate_identifier(&self, expected: &str) -> Result<()> {
        self.validate()?;
        let len = if self.curve == Curve::Secp256k1 {
            40
        } else {
            64
        };
        if expected.len() != len + 2
            || !expected.starts_with("0x")
            || !expected[2..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Failure::Input);
        }
        Ok(())
    }
}
pub fn validate_name(name: &str) -> Result<()> {
    if !wallet_crypto::valid_wallet_name(name) {
        Err(Failure::Input)
    } else {
        Ok(())
    }
}

pub struct DeletePlan {
    pub remaining: Vec<String>,
    pub deleted: usize,
    pub confirmed: bool,
}
impl DeletePlan {
    pub fn new(mut names: Vec<String>) -> Result<Self> {
        if names.is_empty() {
            return Err(Failure::Input);
        }
        for name in &names {
            validate_name(name)?;
        }
        names.sort();
        names.dedup();
        Ok(Self {
            remaining: names,
            deleted: 0,
            confirmed: false,
        })
    }
    pub fn execute(&mut self, mut delete: impl FnMut(&str) -> Result<()>) -> Result<()> {
        if !self.confirmed || self.remaining.is_empty() {
            return Err(Failure::Input);
        }
        self.confirmed = false;
        // Keep the confirmed snapshot; a failure stops before touching any later names.
        while let Some(name) = self.remaining.first() {
            delete(name)?;
            self.remaining.remove(0);
            self.deleted += 1;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct StoredWallet {
    #[zeroize(skip)]
    pub version: u32,
    pub mnemonic: String,
    pub passphrase: String,
}

pub const DEFAULT_ETH_PATH: &str = "m/44'/60'/0'/0/0";
pub const DEFAULT_SOL_PATH: &str = "m/44'/501'/0'/0'";
/// Account indices whose public addresses are cached for display without a key read.
pub const CACHED_ACCOUNTS: u32 = 10;
/// Other derived addresses remembered after an authenticated read, newest last.
pub const MAX_SAVED_ACCOUNTS: usize = 32;

/// A public address derived for a path outside the cached account indices.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SavedAccount {
    pub curve: Curve,
    pub path: String,
    pub address: String,
}

/// The standard account path for an index: the last Ethereum component, or the
/// Solana account component (every Solana component is hardened).
pub fn account_path(curve: Curve, index: u32) -> String {
    match curve {
        Curve::Secp256k1 => format!("m/44'/60'/0'/0/{index}"),
        Curve::Ed25519 => format!("m/44'/501'/{index}'/0'"),
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WalletSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ethereum_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solana_address: Option<String>,
    /// Public addresses for account indices `0..CACHED_ACCOUNTS`, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ethereum_accounts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub solana_accounts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub saved_accounts: Vec<SavedAccount>,
}
impl WalletSummary {
    #[cfg(test)]
    pub fn named(name: String) -> Self {
        Self {
            name,
            ethereum_address: None,
            solana_address: None,
            ethereum_accounts: Vec::new(),
            solana_accounts: Vec::new(),
            saved_accounts: Vec::new(),
        }
    }
    /// Rebuilds the display cache from a Keychain read. `remembered` paths are
    /// re-derived from the wallet, never copied, so a stale or tampered catalog entry
    /// cannot survive a read; paths that no longer validate are dropped.
    pub fn from_wallet(
        name: &str,
        wallet: &StoredWallet,
        remembered: &[SavedAccount],
    ) -> Result<Self> {
        Self::from_seed(name, &WalletSeed::new(wallet)?, remembered)
    }
    /// As `from_wallet`, reusing a seed already computed for this wallet.
    pub fn from_seed(name: &str, seed: &WalletSeed, remembered: &[SavedAccount]) -> Result<Self> {
        let derive = |curve, path: String| {
            seed.derive(
                &KeyRequest {
                    name: name.into(),
                    curve,
                    path,
                },
                None,
            )
        };
        let eth = derive(Curve::Secp256k1, DEFAULT_ETH_PATH.into())?;
        let sol = derive(Curve::Ed25519, DEFAULT_SOL_PATH.into())?;
        let accounts = |curve| {
            (0..CACHED_ACCOUNTS)
                .map(|index| derive(curve, account_path(curve, index)).map(|pair| pair.address))
                .collect::<Result<Vec<_>>>()
        };
        let mut summary = Self {
            name: name.into(),
            ethereum_address: Some(eth.identifier),
            solana_address: Some(sol.address),
            ethereum_accounts: accounts(Curve::Secp256k1)?,
            solana_accounts: accounts(Curve::Ed25519)?,
            saved_accounts: Vec::new(),
        };
        for saved in remembered {
            if let Ok(pair) = derive(saved.curve, saved.path.clone()) {
                summary.remember(saved.curve, &saved.path, &pair.address);
            }
        }
        Ok(summary)
    }
    /// Remembers an authenticated derivation so its address shows without a key read
    /// next time. The oldest entry is dropped beyond `MAX_SAVED_ACCOUNTS`.
    pub fn remember(&mut self, curve: Curve, path: &str, address: &str) {
        if self.cached_account(curve, path).is_some() {
            return;
        }
        self.saved_accounts.push(SavedAccount {
            curve,
            path: path.into(),
            address: address.into(),
        });
        if self.saved_accounts.len() > MAX_SAVED_ACCOUNTS {
            self.saved_accounts.remove(0);
        }
    }
    /// The cached address for a standard account path, if that index is cached.
    pub fn cached_account(&self, curve: Curve, path: &str) -> Option<&str> {
        let accounts = match curve {
            Curve::Secp256k1 => &self.ethereum_accounts,
            Curve::Ed25519 => &self.solana_accounts,
        };
        (0..accounts.len() as u32)
            .position(|index| account_path(curve, index) == path)
            .map(|index| accounts[index].as_str())
            // The default addresses predate the index cache and cover index 0.
            .or_else(|| match curve {
                Curve::Secp256k1 if path == DEFAULT_ETH_PATH => self.ethereum_address.as_deref(),
                Curve::Ed25519 if path == DEFAULT_SOL_PATH => self.solana_address.as_deref(),
                _ => None,
            })
            .or_else(|| {
                self.saved_accounts
                    .iter()
                    .find(|saved| saved.curve == curve && saved.path == path)
                    .map(|saved| saved.address.as_str())
            })
    }
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name).map_err(|_| Failure::Catalog)?;
        let ethereum = |eth: &String| {
            eth.len() == 42
                && eth.starts_with("0x")
                && eth[2..].bytes().all(|b| b.is_ascii_hexdigit())
        };
        let solana = |sol: &String| {
            sol.len() <= 44
                && bs58::decode(sol)
                    .into_vec()
                    .is_ok_and(|bytes| bytes.len() == 32)
        };
        let limit = CACHED_ACCOUNTS as usize;
        let saved = |saved: &SavedAccount| {
            KeyRequest {
                name: self.name.clone(),
                curve: saved.curve,
                path: saved.path.clone(),
            }
            .validate()
            .is_ok()
                && match saved.curve {
                    Curve::Secp256k1 => ethereum(&saved.address),
                    Curve::Ed25519 => solana(&saved.address),
                }
        };
        let has_defaults = self.ethereum_address.is_some();
        let index_zero_matches = |accounts: &[String], default: &Option<String>| {
            accounts
                .first()
                .is_none_or(|first| Some(first) == default.as_ref())
        };
        let unique_paths = self.saved_accounts.iter().enumerate().all(|(i, a)| {
            self.saved_accounts[..i]
                .iter()
                .all(|b| (b.curve, &b.path) != (a.curve, &a.path))
        });
        // Index 0 must agree with the default address shown above it, saved paths are
        // unique, and derived caches only exist alongside the defaults.
        if !index_zero_matches(&self.ethereum_accounts, &self.ethereum_address)
            || !index_zero_matches(&self.solana_accounts, &self.solana_address)
            || !unique_paths
            || (!has_defaults
                && (!self.ethereum_accounts.is_empty()
                    || !self.solana_accounts.is_empty()
                    || !self.saved_accounts.is_empty()))
        {
            return Err(Failure::Catalog);
        }
        if self.ethereum_accounts.len() > limit
            || self.solana_accounts.len() > limit
            || self.saved_accounts.len() > MAX_SAVED_ACCOUNTS
            || !self.ethereum_accounts.iter().all(ethereum)
            || !self.solana_accounts.iter().all(solana)
            || !self.saved_accounts.iter().all(saved)
        {
            return Err(Failure::Catalog);
        }
        match (&self.ethereum_address, &self.solana_address) {
            (None, None) => Ok(()),
            (Some(eth), Some(sol)) if ethereum(eth) && solana(sol) => Ok(()),
            _ => Err(Failure::Catalog),
        }
    }
    pub fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_ascii_lowercase();
        [
            self.name.as_str(),
            self.ethereum_address.as_deref().unwrap_or(""),
            self.solana_address.as_deref().unwrap_or(""),
        ]
        .iter()
        .any(|value| value.to_ascii_lowercase().contains(&query))
    }
}
pub struct Derived {
    pub key: Zeroizing<[u8; 32]>,
    pub public_key: String,
    pub identifier: String,
    pub address: String,
}
pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(2 + bytes.len() * 2);
    result.push_str("0x");
    for byte in bytes {
        let _ = write!(result, "{byte:02x}");
    }
    result
}
pub fn normalize(text: &str) -> Zeroizing<String> {
    Zeroizing::new(text.split_whitespace().collect::<Vec<_>>().join(" "))
}
/// A stored wallet's BIP-39 seed, computed once for several derivations.
pub struct WalletSeed(Seed);
impl WalletSeed {
    pub fn new(wallet: &StoredWallet) -> Result<Self> {
        check_wallet(wallet)?;
        Seed::new(&wallet.mnemonic, &wallet.passphrase)
            .map(Self)
            .map_err(derive_failure)
    }
    pub fn derive(&self, request: &KeyRequest, expected: Option<&str>) -> Result<Derived> {
        request.validate()?;
        if let Some(expected) = expected {
            request.validate_identifier(expected)?;
        }
        let (key, public) = self
            .0
            .derive(request.curve, &request.path)
            .map_err(derive_failure)?;
        let (public_key, address) = match public {
            PublicKey::Secp256k1(public) => (
                hex(&public.compressed_bytes()),
                hex(&public.ethereum_address()),
            ),
            PublicKey::Ed25519(public) => (hex(&public), bs58::encode(public).into_string()),
        };
        let identifier = if request.curve == Curve::Secp256k1 {
            address.clone()
        } else {
            public_key.clone()
        };
        if expected.is_some_and(|value| !identifier.eq_ignore_ascii_case(value)) {
            return Err(if request.curve == Curve::Secp256k1 {
                Failure::Address
            } else {
                Failure::PublicKey
            });
        }
        Ok(Derived {
            key,
            public_key,
            identifier,
            address,
        })
    }
}
fn check_wallet(wallet: &StoredWallet) -> Result<()> {
    if wallet.version != 1 {
        return Err(Failure::Corrupt);
    }
    if wallet.mnemonic.is_empty() || wallet.mnemonic.len() > 4096 || wallet.passphrase.len() > 4096
    {
        return Err(Failure::Input);
    }
    Ok(())
}
fn derive_failure(error: DeriveError) -> Failure {
    match error {
        DeriveError::InvalidMnemonic => Failure::Mnemonic,
        DeriveError::InvalidPath | DeriveError::DerivationFailed => Failure::Derivation,
    }
}

/// A device-local signing key: a random key or a copy derived from a wallet.
pub struct SignerKey {
    pub curve: Curve,
    pub secret: Zeroizing<[u8; 32]>,
}
impl SignerKey {
    /// Keychain encoding. Ed25519 keeps the original raw 32-byte seed so
    /// existing signers stay readable; secp256k1 adds a one-byte tag.
    const SECP256K1_TAG: u8 = 1;
    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let mut data = Zeroizing::new(Vec::with_capacity(33));
        if self.curve == Curve::Secp256k1 {
            data.push(Self::SECP256K1_TAG);
        }
        data.extend_from_slice(&*self.secret);
        data
    }
    pub fn decode(data: &[u8]) -> Result<Self> {
        let (curve, secret) = match (data.len(), data.first()) {
            (32, _) => (Curve::Ed25519, data),
            (33, Some(&Self::SECP256K1_TAG)) => (Curve::Secp256k1, &data[1..]),
            _ => return Err(Failure::Corrupt),
        };
        let key = Self {
            curve,
            secret: Zeroizing::new(secret.try_into().map_err(|_| Failure::Corrupt)?),
        };
        key.validate()?;
        Ok(key)
    }
    pub fn validate(&self) -> Result<()> {
        if self.curve == Curve::Secp256k1
            && wallet_crypto::secp256k1_public_key(&self.secret).is_none()
        {
            return Err(Failure::Corrupt);
        }
        Ok(())
    }
}

/// Copy the key at a wallet path into a standalone signer key.
pub fn derive_signer(wallet: &StoredWallet, curve: Curve, path: &str) -> Result<SignerKey> {
    if !curve.valid_signer_path(path) {
        return Err(Failure::Input);
    }
    check_wallet(wallet)?;
    let (secret, _) = wallet_crypto::derive(&wallet.mnemonic, &wallet.passphrase, curve, path)
        .map_err(derive_failure)?;
    Ok(SignerKey { curve, secret })
}
pub fn derive(
    wallet: &StoredWallet,
    request: &KeyRequest,
    expected: Option<&str>,
) -> Result<Derived> {
    // Cheap checks first, in the original order, before the PBKDF2 stretch.
    request.validate()?;
    check_wallet(wallet)?;
    if let Some(expected) = expected {
        request.validate_identifier(expected)?;
    }
    WalletSeed::new(wallet)?.derive(request, expected)
}
pub fn check_passphrase(passphrase: &str, confirmation: &str, empty: bool) -> Result<()> {
    if passphrase.len() > 4096 || confirmation.len() > 4096 {
        return Err(Failure::Input);
    }
    if if empty {
        passphrase.is_empty() && confirmation.is_empty()
    } else {
        !passphrase.is_empty() && passphrase == confirmation
    } {
        Ok(())
    } else {
        Err(Failure::Input)
    }
}

// Creation requires a matching backup or an explicit choice to skip it.
pub struct PendingWallet {
    wallet: StoredWallet,
    ready_to_save: bool,
}
impl PendingWallet {
    pub fn new(wallet: StoredWallet, creating: bool) -> Self {
        Self {
            wallet,
            ready_to_save: !creating,
        }
    }
    pub fn phrase(&self) -> &str {
        &self.wallet.mnemonic
    }
    pub fn confirm(&mut self, phrase: &str) -> Result<()> {
        self.ready_to_save = *normalize(phrase) == self.wallet.mnemonic;
        if self.ready_to_save {
            Ok(())
        } else {
            Err(Failure::Input)
        }
    }
    pub fn skip_backup(&mut self) {
        self.ready_to_save = true;
    }
    pub fn for_save(&self) -> Result<&StoredWallet> {
        if self.ready_to_save {
            Ok(&self.wallet)
        } else {
            Err(Failure::Input)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const WORDS: &str = "test test test test test test test test test test test junk";
    fn wallet(passphrase: &str) -> StoredWallet {
        StoredWallet {
            version: 1,
            mnemonic: WORDS.into(),
            passphrase: passphrase.into(),
        }
    }
    #[test]
    fn deletion_requires_confirmation_and_stops_at_first_failure() {
        assert!(DeletePlan::new(vec![]).is_err());
        assert!(DeletePlan::new(vec!["invalid name".into()]).is_err());
        let mut plan =
            DeletePlan::new(vec!["c".into(), "a".into(), "b".into(), "a".into()]).unwrap();
        let mut called = Vec::new();
        assert_eq!(
            plan.execute(|_| panic!("unconfirmed deletion")),
            Err(Failure::Input)
        );
        plan.confirmed = true;
        assert_eq!(
            plan.execute(|name| {
                called.push(name.to_owned());
                if name == "b" {
                    Err(Failure::Authentication)
                } else {
                    Ok(())
                }
            }),
            Err(Failure::Authentication)
        );
        assert_eq!(called, ["a", "b"]);
        assert_eq!(plan.deleted, 1);
        assert_eq!(plan.remaining, ["b", "c"]);
        assert!(!plan.confirmed);
        assert_eq!(
            plan.execute(|_| panic!("retry requires new confirmation")),
            Err(Failure::Input)
        );
        plan.confirmed = true;
        plan.execute(|name| {
            called.push(name.to_owned());
            Ok(())
        })
        .unwrap();
        assert_eq!(called, ["a", "b", "b", "c"]);
        assert_eq!(plan.deleted, 3);
        assert!(plan.remaining.is_empty());
        let mut cancelled = DeletePlan::new(vec!["kept".into()]).unwrap();
        cancelled.confirmed = true;
        assert_eq!(
            cancelled.execute(|_| Err(Failure::Cancelled)),
            Err(Failure::Cancelled)
        );
        assert_eq!(cancelled.remaining, ["kept"]);
        assert_eq!(cancelled.deleted, 0);
    }
    fn request(curve: Curve, path: &str) -> KeyRequest {
        KeyRequest {
            name: "public-test".into(),
            curve,
            path: path.into(),
        }
    }
    #[test]
    fn path_name_and_public_identifier_boundaries() {
        for name in ["", "x\n", "-x", "钱包", &"x".repeat(65)] {
            assert_eq!(validate_name(name), Err(Failure::Input));
        }
        for path in ["m", "m/0'/1", "m/2147483648'", "m/0'\n", "m//0'"] {
            assert_eq!(
                request(Curve::Ed25519, path).validate(),
                Err(Failure::Input)
            );
        }
        for path in [
            "m/44'/60'/0'/2/0",
            "m/44'/60'/2147483648'/0/0",
            "m/44'/60'/0'/0/0\n",
        ] {
            assert_eq!(
                request(Curve::Secp256k1, path).validate(),
                Err(Failure::Input)
            );
        }
        assert!(serde_json::from_str::<KeyRequest>(r#"{"name":"x","path":"m/0'"}"#).is_err());
        assert!(
            serde_json::from_str::<KeyRequest>(r#"{"name":"x","path":"m/0'","curve":"p256"}"#)
                .is_err()
        );
        assert!(
            request(Curve::Ed25519, "m/0'")
                .validate_identifier(&format!("0x{}", "0".repeat(64)))
                .is_ok()
        );
        assert!(
            request(Curve::Ed25519, "m/0'")
                .validate_identifier(&format!("0x{}\n", "0".repeat(64)))
                .is_err()
        );
    }
    #[test]
    fn remembered_accounts_are_rederived_not_copied() {
        let stored: StoredWallet = serde_json::from_str(&format!(
            r#"{{"version":1,"mnemonic":"{WORDS}","passphrase":""}}"#
        ))
        .unwrap();
        let saved = |curve, path: &str, address: &str| SavedAccount {
            curve,
            path: path.into(),
            address: address.into(),
        };
        let address = |curve, path| {
            derive(&stored, &request(curve, path), None)
                .unwrap()
                .address
        };
        let eth_path = "m/44'/60'/0'/0/10";
        let sol_path = "m/44'/501'/12'/0'";
        let remembered = [
            // A stale or tampered address is replaced by the derived one.
            saved(Curve::Secp256k1, eth_path, &format!("0x{}", "0".repeat(40))),
            // A path that no longer validates is dropped.
            saved(
                Curve::Ed25519,
                "m/44'/501'/2'/0",
                "oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96",
            ),
            // A path now covered by the index cache is not duplicated.
            saved(
                Curve::Secp256k1,
                "m/44'/60'/0'/0/2",
                "0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc",
            ),
            saved(Curve::Ed25519, sol_path, &address(Curve::Ed25519, sol_path)),
        ];
        let summary = WalletSummary::from_wallet("public-test", &stored, &remembered).unwrap();
        assert_eq!(
            summary.saved_accounts,
            [
                saved(
                    Curve::Secp256k1,
                    eth_path,
                    &address(Curve::Secp256k1, eth_path)
                ),
                saved(Curve::Ed25519, sol_path, &address(Curve::Ed25519, sol_path)),
            ]
        );
        assert!(summary.validate().is_ok());
    }
    #[test]
    fn stored_wallet_and_independent_public_vectors() {
        let stored: StoredWallet = serde_json::from_str(&format!(
            r#"{{"version":1,"mnemonic":"{WORDS}","passphrase":""}}"#
        ))
        .unwrap();
        let pair = derive(
            &stored,
            &request(Curve::Secp256k1, "m/44'/60'/0'/0/0"),
            Some("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266"),
        )
        .unwrap();
        assert_eq!(pair.key.len(), 32);
        assert_eq!(pair.public_key.len(), 68);
        assert!(pair.public_key.starts_with("0x02") || pair.public_key.starts_with("0x03"));
        let summary = WalletSummary::from_wallet("public-test", &stored, &[]).unwrap();
        assert_eq!(
            summary.ethereum_address.as_deref(),
            Some("0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266")
        );
        assert_eq!(
            summary.solana_address.as_deref(),
            Some("oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96")
        );
        assert_eq!(
            bs58::decode(summary.solana_address.as_ref().unwrap())
                .into_vec()
                .unwrap(),
            [
                0x0b, 0xf3, 0x2b, 0x9f, 0x0d, 0xb0, 0x96, 0x72, 0x03, 0x8f, 0xea, 0x36, 0x13, 0x9b,
                0x18, 0xf9, 0x8a, 0x5f, 0x01, 0x49, 0xef, 0x4c, 0xe0, 0x33, 0x2e, 0x44, 0xb9, 0xa7,
                0x7e, 0x83, 0xc2, 0x2d,
            ]
        );
        assert!(summary.validate().is_ok());
        assert_eq!(summary.ethereum_accounts.len(), CACHED_ACCOUNTS as usize);
        assert_eq!(summary.solana_accounts.len(), CACHED_ACCOUNTS as usize);
        assert_eq!(
            summary.cached_account(Curve::Secp256k1, "m/44'/60'/0'/0/2"),
            Some("0x3c44cdddb6a900fa2b585dd299e03d12fa4293bc")
        );
        assert_eq!(
            summary.cached_account(Curve::Ed25519, "m/44'/501'/0'/0'"),
            Some("oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96")
        );
        assert_eq!(
            summary.cached_account(Curve::Secp256k1, "m/44'/60'/0'/0/10"),
            None
        );
        assert_eq!(
            summary.cached_account(Curve::Ed25519, "m/44'/501'/2'/0"),
            None
        );
        let mut tampered = summary.clone();
        tampered.solana_accounts[3] = "not-an-address".into();
        assert!(tampered.validate().is_err());
        let mut remembered = summary.clone();
        remembered.remember(
            Curve::Secp256k1,
            "m/44'/60'/0'/0/2",
            "0xignored0000000000000000000000000000000000",
        );
        assert!(
            remembered.saved_accounts.is_empty(),
            "cached indices are not saved twice"
        );
        remembered.remember(
            Curve::Ed25519,
            "m/44'/501'/0'/1'",
            "oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96",
        );
        assert_eq!(
            remembered.cached_account(Curve::Ed25519, "m/44'/501'/0'/1'"),
            Some("oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96")
        );
        assert!(remembered.validate().is_ok());
        for index in 100..100 + MAX_SAVED_ACCOUNTS as u32 {
            remembered.remember(
                Curve::Secp256k1,
                &account_path(Curve::Secp256k1, index),
                "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266",
            );
        }
        assert_eq!(remembered.saved_accounts.len(), MAX_SAVED_ACCOUNTS);
        assert!(
            remembered
                .cached_account(Curve::Ed25519, "m/44'/501'/0'/1'")
                .is_none()
        );
        assert!(remembered.validate().is_ok());
        remembered.saved_accounts[0].path = "m/44'/501'/0'/1".into();
        assert!(remembered.validate().is_err());
        let mut mismatched = summary.clone();
        mismatched.ethereum_accounts[0] = mismatched.ethereum_accounts[1].clone();
        assert!(
            mismatched.validate().is_err(),
            "index 0 must match the default"
        );
        let mut duplicated = summary.clone();
        duplicated.remember(
            Curve::Ed25519,
            "m/44'/501'/0'/1'",
            "oeYf6KAJkLYhBuR8CiGc6L4D4Xtfepr85fuDgA9kq96",
        );
        assert!(duplicated.validate().is_ok());
        duplicated
            .saved_accounts
            .push(duplicated.saved_accounts[0].clone());
        assert!(duplicated.validate().is_err(), "saved paths must be unique");
        let mut orphaned = summary.clone();
        orphaned.ethereum_address = None;
        orphaned.solana_address = None;
        assert!(
            orphaned.validate().is_err(),
            "caches need default addresses"
        );
        let mut oversized = summary.clone();
        oversized
            .ethereum_accounts
            .push(oversized.ethereum_accounts[0].clone());
        assert!(oversized.validate().is_err());
        assert!(summary.matches(" PUBLIC-TEST "));
        assert!(summary.matches("f39fd6e5"));
        assert!(summary.matches("oeYf6KAJ"));
        assert!(summary.matches(""));
        assert!(!summary.matches("not-an-account"));
        for (passphrase, path, public) in [
            (
                "",
                "m/44'/501'/0'/0'",
                "0bf32b9f0db09672038fea36139b18f98a5f0149ef4ce0332e44b9a77e83c22d",
            ),
            (
                "",
                "m/44'/501'/1'/0'",
                "9245441f4a752ea6328c6d8d2d2afcb36d131db998a47ca04d6d5d8587d0b012",
            ),
            (
                "TREZOR",
                "m/44'/501'/0'/0'",
                "0bd89cf16f4f7a40a2839b8ca08832cebec6737bd30cae4af528e59e6c6b0f33",
            ),
            (
                "é",
                "m/44'/501'/0'/0'",
                "e0d2365114a62d771831bed83e660ef8d88c0b35bf9a166bc98e1f32b8255254",
            ),
            (
                "e\u{301}",
                "m/44'/501'/0'/0'",
                "e0d2365114a62d771831bed83e660ef8d88c0b35bf9a166bc98e1f32b8255254",
            ),
        ] {
            assert_eq!(
                derive(
                    &wallet(passphrase),
                    &request(Curve::Ed25519, path),
                    Some(&format!("0x{public}"))
                )
                .unwrap()
                .public_key,
                format!("0x{public}")
            );
        }
        assert!(matches!(
            derive(
                &stored,
                &request(Curve::Ed25519, "m/0'"),
                Some(&format!("0x{}", "0".repeat(64)))
            ),
            Err(Failure::PublicKey)
        ));
        assert!(matches!(
            derive(
                &wallet(" "),
                &request(Curve::Secp256k1, "m/44'/60'/0'/0/0"),
                Some(&pair.identifier)
            ),
            Err(Failure::Address)
        ));
    }
    #[test]
    fn backup_gate_and_explicit_passphrase() {
        assert!(check_passphrase("", "", false).is_err());
        assert!(check_passphrase(" ", " ", false).is_ok());
        assert!(check_passphrase(" ", " ", true).is_err());
        assert!(check_passphrase("", "", true).is_ok());
        assert!(check_passphrase("x", "y", false).is_err());
        let mut pending = PendingWallet::new(wallet(""), true);
        assert!(pending.for_save().is_err());
        assert!(pending.confirm("wrong backup").is_err());
        assert!(pending.for_save().is_err());
        assert!(pending.confirm(&format!("  {WORDS}\n")).is_ok());
        assert!(pending.for_save().is_ok());
        assert!(pending.confirm("wrong backup").is_err());
        assert!(pending.for_save().is_err());
    }

    #[test]
    fn explicit_backup_skip_preserves_wallet_and_does_not_authorize_other_drafts() {
        let mut pending = PendingWallet::new(wallet("test-passphrase"), true);
        assert!(pending.for_save().is_err());
        pending.skip_backup();
        let saved = pending.for_save().unwrap();
        assert_eq!(saved.mnemonic, WORDS);
        assert_eq!(saved.passphrase, "test-passphrase");
        assert!(PendingWallet::new(wallet(""), true).for_save().is_err());
        assert!(pending.confirm("wrong backup").is_err());
        assert!(pending.for_save().is_err());
    }
}
