use thiserror::Error;

/// A Keylet client failure with a stable code for the SDK bindings.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum ClientError {
    /// The client rejected the wallet selection.
    #[error("Invalid wallet selection")]
    InvalidWalletSelection,
    /// The client rejected the signer request.
    #[error("Invalid signer request")]
    InvalidSignerRequest,
    /// The app response was missing or invalid.
    #[error("INVALID_HELPER_RESPONSE")]
    InvalidHelperResponse,
    /// Launch Services could not find Keylet.
    #[error("KEYLET_APP_NOT_FOUND")]
    AppNotFound,
    /// The registered app could not be started.
    #[error("HELPER_START_FAILED")]
    HelperStartFailed,
    /// The app exited without a recognized error code.
    #[error("HELPER_FAILED")]
    HelperFailed,
    /// The app did not respond before the deadline.
    #[error("AUTHENTICATION_TIMEOUT")]
    AuthenticationTimeout,
    /// The request was cancelled.
    #[error("CANCELLED")]
    Cancelled,
    /// The app could not authenticate the user.
    #[error("AUTHENTICATION_FAILED")]
    AuthenticationFailed,
    /// The requested key or wallet was not found.
    #[error("NOT_FOUND")]
    NotFound,
    /// An entry with the same name already exists.
    #[error("ALREADY_EXISTS")]
    AlreadyExists,
    /// The Keychain or its client session is unavailable.
    #[error("KEYCHAIN_ERROR")]
    Keychain,
    /// The app signing configuration is invalid.
    #[error("SIGNING_CONFIGURATION")]
    SigningConfiguration,
    /// The app could not load its settings.
    #[error("SETTINGS_ERROR")]
    Settings,
    /// The app rejected the request.
    #[error("INVALID_INPUT")]
    InvalidInput,
    /// The app rejected the recovery phrase.
    #[error("INVALID_MNEMONIC")]
    InvalidMnemonic,
    /// The app could not derive a key.
    #[error("DERIVATION_FAILED")]
    DerivationFailed,
    /// The derived address did not match the expected address.
    #[error("ADDRESS_MISMATCH")]
    AddressMismatch,
    /// The public key did not match the expected key.
    #[error("PUBLIC_KEY_MISMATCH")]
    PublicKeyMismatch,
    /// Communication with the app failed.
    #[error("TRANSPORT_ERROR")]
    Transport,
    /// The app could not read the stored item.
    #[error("INVALID_ITEM")]
    InvalidItem,
    /// The app encountered an internal failure.
    #[error("INTERNAL_ERROR")]
    Internal,
}

pub(super) fn reported_error(bytes: &[u8]) -> ClientError {
    let message = std::str::from_utf8(bytes).unwrap_or("").trim();
    [
        ClientError::Cancelled,
        ClientError::AuthenticationFailed,
        ClientError::NotFound,
        ClientError::AlreadyExists,
        ClientError::Keychain,
        ClientError::SigningConfiguration,
        ClientError::Settings,
        ClientError::InvalidInput,
        ClientError::InvalidMnemonic,
        ClientError::DerivationFailed,
        ClientError::AddressMismatch,
        ClientError::PublicKeyMismatch,
        ClientError::Transport,
        ClientError::InvalidItem,
        ClientError::Internal,
    ]
    .into_iter()
    .find(|error| error.to_string() == message)
    .unwrap_or(ClientError::HelperFailed)
}
