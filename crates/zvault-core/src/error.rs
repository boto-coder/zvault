//! Unified error and result types for `zvault-core`.
//!
//! All fallible operations in this crate return [`Result<T>`], which is an
//! alias for `std::result::Result<T, Error>`.

use thiserror::Error;
use uuid::Uuid;

/// Unified error type for all `zvault-core` operations.
#[derive(Debug, Error)]
pub enum Error {
    /// A cryptographic operation failed (bad key, bad tag, KDF error, etc.)
    #[error("crypto error: {0}")]
    Crypto(String),

    /// An I/O error from the underlying filesystem or network.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON or MessagePack serialisation / deserialisation failure.
    #[error("serialisation error: {0}")]
    Serialisation(String),

    /// The vault file header is corrupt, truncated, has wrong magic bytes,
    /// or the AES-GCM authentication tag does not match.
    #[error("invalid vault file: {0}")]
    InvalidVaultFile(String),

    /// The referenced device ID is not in the authorised device list.
    #[error("device not found: {0}")]
    DeviceNotFound(Uuid),

    /// The referenced device has been revoked and cannot perform this operation.
    #[error("device revoked: {0}")]
    DeviceRevoked(Uuid),

    /// A Nostr relay / WebSocket / sync-protocol error.
    #[error("sync error: {0}")]
    SyncError(String),

    /// A UTF-8 decoding error.
    #[error("utf-8 error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),

    /// A base64 decoding error.
    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
}

/// Convenience `Result` alias for `zvault-core`.
pub type Result<T> = std::result::Result<T, Error>;
