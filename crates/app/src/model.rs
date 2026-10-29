use serde::{Deserialize, Serialize};
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
            Self::Settings => "SETTINGS_ERROR",
        }
    }
    pub fn message(self) -> &'static str {
        match self {
            Self::Settings => "Unable to read authentication settings. Open Settings and save an option.",
            Self::Mnemonic => "Check the English recovery words, their order, and checksum.",
            Self::Address => "The phrase, passphrase, and path do not match the expected Ethereum address.",
            Self::PublicKey => "The phrase, passphrase, and path do not match the expected Ed25519 public key.",
            Self::Duplicate => "That name already exists. The existing wallet was not changed.",
            Self::Configuration => "The app's signing or secure-input configuration is invalid. Rebuild with your Personal Team.",
            Self::Authentication => "macOS authentication was not completed. Nothing was unlocked.",
            _ => "Check the wallet details, derivation path, and matching passphrase fields. Nothing was saved.",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Curve {
    #[serde(rename = "secp256k1")]
    Secp256k1,
    #[serde(rename = "ed25519")]
    Ed25519,
}
impl Curve {
    pub fn name(self) -> &'static str {
        match self {
            Self::Secp256k1 => "secp256k1",
            Self::Ed25519 => "ed25519",
        }
    }
    pub fn id(self) -> u32 {
        match self {
            Self::Secp256k1 => 0,
            Self::Ed25519 => 1,
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
        if self.path.len() > 128 {
            return Err(Failure::Input);
        }
        let parts: Vec<_> = self.path.split('/').collect();
        let number = |s: &str| {
            !s.is_empty()
                && s.bytes().all(|b| b.is_ascii_digit())
                && s.parse::<u32>().is_ok_and(|n| n < 0x80000000)
        };
        let hardened = |s: &str| s.strip_suffix('\'').is_some_and(number);
        let valid = match self.curve {
            Curve::Secp256k1 => {
                parts.len() == 6
                    && parts[..3] == ["m", "44'", "60'"]
                    && hardened(parts[3])
                    && matches!(parts[4], "0" | "1")
                    && number(parts[5])
            }
            Curve::Ed25519 => {
                parts.len() >= 2 && parts[0] == "m" && parts[1..].iter().all(|s| hardened(s))
            }
        };
        if valid {
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
    if name.is_empty()
        || name.len() > 64
        || !name.as_bytes()[0].is_ascii_alphanumeric()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WalletSummary {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ethereum_address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solana_address: Option<String>,
}
impl WalletSummary {
    #[cfg(test)]
    pub fn named(name: String) -> Self {
        Self {
            name,
            ethereum_address: None,
            solana_address: None,
        }
    }
    pub fn from_wallet(name: &str, wallet: &StoredWallet) -> Result<Self> {
        let eth = derive(
            wallet,
            &KeyRequest {
                name: name.into(),
                curve: Curve::Secp256k1,
                path: DEFAULT_ETH_PATH.into(),
            },
            None,
        )?;
        let (_seed, sol) =
            wallet_crypto::derive(&wallet.mnemonic, &wallet.passphrase, 1, DEFAULT_SOL_PATH)
                .map_err(|_| Failure::Derivation)?;
        Ok(Self {
            name: name.into(),
            ethereum_address: Some(eth.identifier),
            solana_address: Some(bs58::encode(sol).into_string()),
        })
    }
    pub fn validate(&self) -> Result<()> {
        validate_name(&self.name).map_err(|_| Failure::Catalog)?;
        match (&self.ethereum_address, &self.solana_address) {
            (None, None) => Ok(()),
            (Some(eth), Some(sol))
                if eth.len() == 42
                    && eth.starts_with("0x")
                    && eth[2..].bytes().all(|b| b.is_ascii_hexdigit())
                    && sol.len() <= 44
                    && bs58::decode(sol)
                        .into_vec()
                        .is_ok_and(|bytes| bytes.len() == 32) =>
            {
                Ok(())
            }
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
pub fn derive(
    wallet: &StoredWallet,
    request: &KeyRequest,
    expected: Option<&str>,
) -> Result<Derived> {
    request.validate()?;
    if wallet.version != 1 {
        return Err(Failure::Corrupt);
    }
    if wallet.mnemonic.is_empty() || wallet.mnemonic.len() > 4096 || wallet.passphrase.len() > 4096
    {
        return Err(Failure::Input);
    }
    if let Some(expected) = expected {
        request.validate_identifier(expected)?;
    }
    let (key, public) = wallet_crypto::derive(
        &wallet.mnemonic,
        &wallet.passphrase,
        request.curve.id(),
        &request.path,
    )
    .map_err(|code| {
        if code == 1 {
            Failure::Mnemonic
        } else {
            Failure::Derivation
        }
    })?;
    let public_key = if request.curve == Curve::Secp256k1 {
        hex(&wallet_crypto::compressed_public_key(&public).map_err(|_| Failure::Derivation)?)
    } else {
        hex(&public)
    };
    let address = match request.curve {
        Curve::Secp256k1 => hex(&wallet_crypto::ethereum_address(&public)),
        Curve::Ed25519 => bs58::encode(&public).into_string(),
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
        assert!(request(Curve::Ed25519, "m/0'")
            .validate_identifier(&format!("0x{}", "0".repeat(64)))
            .is_ok());
        assert!(request(Curve::Ed25519, "m/0'")
            .validate_identifier(&format!("0x{}\n", "0".repeat(64)))
            .is_err());
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
        let summary = WalletSummary::from_wallet("public-test", &stored).unwrap();
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
