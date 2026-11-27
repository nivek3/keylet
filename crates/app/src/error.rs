pub use wallet_crypto::error::HelperError as Failure;

pub type Result<T> = std::result::Result<T, Failure>;

pub fn failure_message(error: Failure) -> &'static str {
    match error {
        Failure::Settings => {
            "Unable to read authentication settings. Open Settings and save an option."
        }
        Failure::Mnemonic => "Check the seed phrase words and their order.",
        Failure::Address => {
            "The seed phrase, passphrase, or path doesn't match this Ethereum address."
        }
        Failure::PublicKey => {
            "The seed phrase, passphrase, or path doesn't match this Ed25519 public key."
        }
        Failure::Duplicate => "That name already exists. The existing wallet was not changed.",
        Failure::Configuration => {
            "The app's signing or secure-input configuration is invalid. Rebuild with your Personal Team."
        }
        Failure::Authentication => "macOS authentication was not completed. Nothing was unlocked.",
        _ => "Check the name, path, and passphrase. Nothing was saved.",
    }
}
