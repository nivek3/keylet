use thiserror::Error;
use wallet_crypto::error::HelperError;

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
    /// An error code from the helper protocol.
    #[error(transparent)]
    Helper(#[from] HelperError),
}

pub(super) fn reported_error(bytes: &[u8]) -> ClientError {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|message| message.trim().parse::<HelperError>().ok())
        .map_or(ClientError::HelperFailed, ClientError::from)
}
