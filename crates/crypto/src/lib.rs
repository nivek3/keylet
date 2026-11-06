//! Shared wallet derivation and selection rules.
use bip32::{secp256k1::ecdsa::VerifyingKey, DerivationPath, XPrv};
use bip39::{Language, Mnemonic};
use derivation_path::DerivationPath as EdPath;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use solana_ed25519::ed_sigs::ExtendedSigningKey;
use std::fmt;
use zeroize::Zeroizing;

/// The two curves supported by the app and the Node bridge.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Curve {
    #[serde(rename = "secp256k1")]
    Secp256k1,
    #[serde(rename = "ed25519")]
    Ed25519,
}

impl Curve {
    /// The curve identifier used by the bridge protocol.
    pub fn name(self) -> &'static str {
        match self {
            Self::Secp256k1 => "secp256k1",
            Self::Ed25519 => "ed25519",
        }
    }

    /// Check a wallet path against the rules for this curve.
    pub fn valid_wallet_path(self, path: &str) -> bool {
        if path.len() > 128 {
            return false;
        }
        let Some(path) = path.strip_prefix("m/") else {
            return false;
        };
        let mut parts = path.split('/');
        let number = |text: &str| {
            !text.is_empty()
                && text.bytes().all(|byte| byte.is_ascii_digit())
                && text.parse::<u32>().is_ok_and(|index| index < 0x80000000)
        };
        let hardened = |text: &str| text.strip_suffix('\'').is_some_and(number);
        match self {
            Self::Secp256k1 => {
                parts.next() == Some("44'")
                    && parts.next() == Some("60'")
                    && parts.next().is_some_and(hardened)
                    && matches!(parts.next(), Some("0" | "1"))
                    && parts.next().is_some_and(number)
                    && parts.next().is_none()
            }
            Self::Ed25519 => parts.all(hardened),
        }
    }
}

/// Check the account name accepted by both the app and the Node bridge.
pub fn valid_wallet_name(name: &str) -> bool {
    name.len() <= 64
        && name
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_alphanumeric)
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

/// A derivation failure without secret-bearing context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeriveError {
    InvalidMnemonic,
    InvalidPath,
    DerivationFailed,
}

impl fmt::Display for DeriveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidMnemonic => "invalid mnemonic",
            Self::InvalidPath => "invalid derivation path",
            Self::DerivationFailed => "key derivation failed",
        })
    }
}

impl std::error::Error for DeriveError {}

/// A public key in the format appropriate to its curve.
#[derive(Debug)]
pub enum PublicKey {
    Secp256k1(EvmPublicKey),
    Ed25519([u8; 32]),
}

/// A validated secp256k1 public key.
#[derive(Debug)]
pub struct EvmPublicKey(VerifyingKey);

impl EvmPublicKey {
    /// Derive the Ethereum address from the uncompressed coordinates.
    pub fn ethereum_address(&self) -> [u8; 20] {
        let point = self.0.to_sec1_point(false);
        let hash = Keccak256::digest(&point.as_bytes()[1..]);
        let mut address = [0; 20];
        address.copy_from_slice(&hash[12..]);
        address
    }

    /// Encode the key in compressed SEC1 form for the SDK.
    pub fn compressed_bytes(&self) -> [u8; 33] {
        let point = self.0.to_sec1_point(true);
        let mut bytes = [0; 33];
        bytes.copy_from_slice(point.as_bytes());
        bytes
    }
}

fn derive_seed(
    seed: &[u8],
    curve: Curve,
    path: &str,
) -> Result<(Zeroizing<[u8; 32]>, PublicKey), DeriveError> {
    match curve {
        Curve::Secp256k1 => {
            let path: DerivationPath = path.parse().map_err(|_| DeriveError::InvalidPath)?;
            let child =
                XPrv::derive_from_path(seed, &path).map_err(|_| DeriveError::DerivationFailed)?;
            Ok((
                Zeroizing::new(child.to_bytes()),
                PublicKey::Secp256k1(EvmPublicKey(*child.private_key().verifying_key())),
            ))
        }
        Curve::Ed25519 => {
            let path: EdPath = path.parse().map_err(|_| DeriveError::InvalidPath)?;
            // SLIP-0010 rejects non-hardened children.
            let child = ExtendedSigningKey::from_seed(seed)
                .derive_path(path.into_iter().map(|index| index.to_bits()))
                .map_err(|_| DeriveError::DerivationFailed)?;
            Ok((
                Zeroizing::new(child.signing_key().to_secret_key_bytes()),
                PublicKey::Ed25519(child.signing_key().verification_key().into()),
            ))
        }
    }
}

/// Derive the selected private key and its public key.
///
/// # Errors
///
/// Returns an error if the mnemonic or path is invalid or derivation fails.
pub fn derive(
    mnemonic: &str,
    passphrase: &str,
    curve: Curve,
    path: &str,
) -> Result<(Zeroizing<[u8; 32]>, PublicKey), DeriveError> {
    let mnemonic = Mnemonic::parse_in(Language::English, mnemonic)
        .map_err(|_| DeriveError::InvalidMnemonic)?;
    let seed = Zeroizing::new(mnemonic.to_seed(passphrase));
    derive_seed(&seed[..], curve, path)
}

/// Convert OS-supplied entropy to an English BIP-39 phrase.
///
/// # Errors
///
/// Returns an error if the BIP-39 backend rejects the entropy.
pub fn mnemonic_from_entropy(entropy: &[u8; 16]) -> Result<Zeroizing<String>, DeriveError> {
    Mnemonic::from_entropy_in(Language::English, entropy)
        .map(|m| Zeroizing::new(m.to_string()))
        .map_err(|_| DeriveError::InvalidMnemonic)
}

#[cfg(test)]
mod tests {
    use super::*;
    const WORDS: &str = "test test test test test test test test test test test junk";
    const PATH: &str = "m/44'/60'/0'/0/0";

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    #[test]
    fn slip10_official_ed25519_vector() {
        // SLIP-0010 test vector 1: master and final hardened child.
        let seed = bytes("000102030405060708090a0b0c0d0e0f");
        for (path, secret, public) in [
            (
                "m",
                "2b4be7f19ee27bbf30c667b642d5f4aa69fd169872f8fc3059c08ebae2eb19e7",
                "a4b2856bfec510abab89753fac1ac0e1112364e7d250545963f135f2a33188ed",
            ),
            (
                "m/0'/1'/2'/2'/1000000000'",
                "8f94d394a8e8fd6b1bc2f3f49f5c47e385281d5c17e65324b0f62483e37e8793",
                "3c24da049451555d51a7014a37337aa4e12d41e485abccfa46b47dfb2af54b7a",
            ),
        ] {
            let (key, result) = derive_seed(&seed, Curve::Ed25519, path).unwrap();
            assert_eq!(key.as_slice(), bytes(secret));
            let PublicKey::Ed25519(result) = result else {
                panic!("SLIP-0010 must return an Ed25519 public key");
            };
            assert_eq!(result.as_slice(), bytes(public));
        }
        assert!(derive_seed(&seed, Curve::Ed25519, "m/0'/1").is_err());
        assert!(derive_seed(&seed, Curve::Ed25519, "m/2147483648'").is_err());
        assert!(derive_seed(&seed, Curve::Ed25519, "bad/path").is_err());
    }

    #[test]
    fn ed25519_extended_keys_clear_secret_and_chain_code() {
        use zeroize::Zeroize;
        assert!(std::mem::needs_drop::<ExtendedSigningKey>());
        let mut key = ExtendedSigningKey::from_seed(&[1; 32]);
        key.zeroize();
        assert_eq!(key.signing_key().as_secret_key_bytes(), &[0; 32]);
        assert_eq!(key.chain_code(), &[0; 32]);
    }

    #[test]
    fn solana_derivation_matches_previous_backend() {
        // Captured from ed25519-dalek-bip32 using the public Hardhat mnemonic.
        for (passphrase, path, secret, public) in [
            (
                "",
                "m/44'/501'/0'/0'",
                "a01c67efb5cb8f62f15907a9fcf124c73d2695c8a9a31f21c05742a31ca1dc72",
                "0bf32b9f0db09672038fea36139b18f98a5f0149ef4ce0332e44b9a77e83c22d",
            ),
            (
                "",
                "m/44'/501'/1'/0'",
                "3f25626cf245c8dd65a2b91a70e6f4c285182489fda230e2553c3752aa962b61",
                "9245441f4a752ea6328c6d8d2d2afcb36d131db998a47ca04d6d5d8587d0b012",
            ),
            (
                "",
                "m/2147483647'",
                "ec130d14b2b636887c6c9b91afc937d0e84fe4fde149e2e9766aa926f9830c63",
                "1946234b8b7c825e1d9f38251af3cd6edd8dfd80f8b41bff84a48dbe28a1348e",
            ),
            (
                "TREZOR",
                "m/44'/501'/0'/0'",
                "81323bfedeabf1da2af3fc3d5370be76862f3e4a0e674efb0e3ddc81c1b6ed5f",
                "0bd89cf16f4f7a40a2839b8ca08832cebec6737bd30cae4af528e59e6c6b0f33",
            ),
        ] {
            let (key, result) = derive(WORDS, passphrase, Curve::Ed25519, path).unwrap();
            assert_eq!(key.as_slice(), bytes(secret));
            let PublicKey::Ed25519(result) = result else {
                panic!("SLIP-0010 must return an Ed25519 public key");
            };
            assert_eq!(result.as_slice(), bytes(public));
        }
    }

    #[test]
    fn entropy_to_mnemonic_bip39_vector() {
        let entropy = [0u8; 16];
        let words = mnemonic_from_entropy(&entropy).unwrap();
        assert_eq!(words.as_str(), "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about");
        assert_eq!(
            Mnemonic::parse_in(Language::English, words.as_str())
                .unwrap()
                .to_entropy(),
            entropy
        );
    }

    #[test]
    fn public_hardhat_vector() {
        let (_, public) = derive(WORDS, "", Curve::Secp256k1, PATH).unwrap();
        let PublicKey::Secp256k1(public) = public else {
            panic!("the Ethereum vector must return a secp256k1 public key");
        };
        let address = public.ethereum_address();
        assert_eq!(
            address,
            [
                0xf3, 0x9f, 0xd6, 0xe5, 0x1a, 0xad, 0x88, 0xf6, 0xf4, 0xce, 0x6a, 0xb8, 0x82, 0x72,
                0x79, 0xcf, 0xff, 0xb9, 0x22, 0x66
            ]
        );
        let compressed = public.compressed_bytes();
        assert_eq!(compressed.len(), 33);
        assert!(matches!(compressed[0], 2 | 3));
        let restored = bip32::secp256k1::ecdsa::VerifyingKey::from_sec1_bytes(&compressed)
            .unwrap()
            .to_sec1_point(false);
        assert_eq!(restored, public.0.to_sec1_point(false));
    }

    #[test]
    fn bip39_passphrase_vector_and_nfkd() {
        let words = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
        let mnemonic = Mnemonic::parse_in(Language::English, words).unwrap();
        let seed = Zeroizing::new(mnemonic.to_seed("TREZOR"));
        let hex: String = seed.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e53495531f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04");
        assert_eq!(
            *derive(WORDS, "é", Curve::Secp256k1, PATH).unwrap().0,
            *derive(WORDS, "e\u{301}", Curve::Secp256k1, PATH).unwrap().0
        );
        assert_ne!(
            *derive(WORDS, "", Curve::Secp256k1, PATH).unwrap().0,
            *derive(WORDS, " ", Curve::Secp256k1, PATH).unwrap().0
        );
    }

    #[test]
    fn rejects_bad_checksum_and_path() {
        assert!(derive("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon", "", Curve::Secp256k1, PATH).is_err());
        assert!(derive(WORDS, "", Curve::Secp256k1, "bad/path").is_err());
    }
}
