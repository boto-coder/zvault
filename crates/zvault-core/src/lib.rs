//! # zvault-core
//!
//! Platform-agnostic core library for ZVault.
//!
//! ## Module layout
//!
//! - [`crypto`]  — Argon2id KDF, AES-256-GCM vault encryption/decryption, key zeroing
//! - [`vault`]   — Data model (`Vault`, `VaultItem`, `DeviceEntry`), serialisation, CRUD
//! - [`device`]  — Device keypair generation, secure storage abstraction, admit/revoke
//! - [`nostr`]   — Nostr keypair, event signing, NIP-44 encryption, NIP-59 gift-wrap
//! - [`sync`]    — Sync engine: message construction, Lamport clock, conflict resolution
//! - [`audit`]   — Audit log, HMAC hash chain, chain verification
//! - [`import`]  — Parsers for Bitwarden JSON, generic CSV, `.zvault-export`
//! - [`export`]  — Writers for `.zvault-export`, plaintext JSON, plaintext CSV
//! - [`error`]   — Unified error type
//!
//! ## Feature flags
//!
//! - `biometric` *(default)* — compile biometric-unlock helpers. Disable on platforms
//!   that do not have a secure enclave or OS keychain.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![warn(clippy::all, clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
// ZVault is a proper noun; suppress the doc_markdown lint that wants backticks.
#![allow(clippy::doc_markdown)]

pub mod audit;
pub mod crypto;
pub mod device;
pub mod error;
pub mod export;
pub mod import;
pub mod nip19;
pub mod nostr;
/// Nostr relay WebSocket transport (requires tokio runtime).
#[cfg(feature = "native")]
pub mod relay;
pub mod settings;
pub mod sync;
pub mod vault;

pub use error::{Error, Result};
