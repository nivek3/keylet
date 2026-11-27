//! Errors for wallet derivation and the app's helper protocol.

use strum::{Display, EnumString};
use thiserror::Error;

/// An error code written by the app to stderr and recognized by the SDK.
#[derive(Clone, Copy, Debug, Display, EnumString, Eq, Error, PartialEq)]
pub enum HelperError {
    /// The user cancelled the operation.
    #[strum(serialize = "CANCELLED")]
    Cancelled,
    /// macOS did not authenticate the user.
    #[strum(serialize = "AUTHENTICATION_FAILED")]
    Authentication,
    /// The requested entry does not exist.
    #[strum(serialize = "NOT_FOUND")]
    Missing,
    /// An entry with this name already exists.
    #[strum(serialize = "ALREADY_EXISTS")]
    Duplicate,
    /// The Keychain operation failed.
    #[strum(serialize = "KEYCHAIN_ERROR")]
    Keychain,
    /// The signing or secure-input configuration is invalid.
    #[strum(serialize = "SIGNING_CONFIGURATION")]
    Configuration,
    /// The app rejected the request.
    #[strum(serialize = "INVALID_INPUT")]
    Input,
    /// The app rejected the recovery phrase.
    #[strum(serialize = "INVALID_MNEMONIC")]
    Mnemonic,
    /// The app could not derive a key.
    #[strum(serialize = "DERIVATION_FAILED")]
    Derivation,
    /// The public key differs from the expected key.
    #[strum(serialize = "PUBLIC_KEY_MISMATCH")]
    PublicKey,
    /// The address differs from the expected address.
    #[strum(serialize = "ADDRESS_MISMATCH")]
    Address,
    /// Communication with the app failed.
    #[strum(serialize = "TRANSPORT_ERROR")]
    Transport,
    /// The app could not read the stored item.
    #[strum(serialize = "INVALID_ITEM")]
    Corrupt,
    /// The wallet list could not be read or written.
    #[strum(serialize = "NAME_LIST_ERROR")]
    Catalog,
    /// The wallet list changed during the operation.
    #[strum(serialize = "NAME_LIST_CHANGED")]
    CatalogChanged,
    /// The app could not load or save its settings.
    #[strum(serialize = "SETTINGS_ERROR")]
    Settings,
    /// The app encountered an internal failure.
    #[strum(serialize = "INTERNAL_ERROR")]
    Internal,
}

/// A derivation failure without secret-bearing context.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum DeriveError {
    /// The phrase is not a valid English BIP-39 mnemonic (words or checksum).
    #[error("invalid mnemonic")]
    InvalidMnemonic,
    /// The derivation path does not parse for the selected curve.
    #[error("invalid derivation path")]
    InvalidPath,
    /// The backend rejected a derivation step.
    #[error("key derivation failed")]
    DerivationFailed,
}
