# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] — Unreleased

### Added

- **M12 — Hardening & Release Prep**
  - Fuzz testing targets (`cargo-fuzz`): decrypt, vault parser, NIP-44 decrypt, CSV import, Bitwarden import
  - Property-based tests (`proptest`): encrypt/decrypt roundtrip, NIP-44 roundtrip, vault CRUD invariants
  - Release automation workflow (multi-platform binary builds on tag push)
  - `SECURITY.md` with vulnerability reporting instructions
  - This changelog

## [0.12.0] — 2026-08-13

### Added

- **M11 — CLI Tool**
  - `zvault-cli` binary with `clap` subcommands
  - Commands: `create`, `open`, `lock`, `list`, `add`, `edit`, `delete`, `export`, `import`, `sync`, `audit`
  - Interactive password prompt and `ZVAULT_PASSWORD` env var support
  - JSON output mode for scripting (`--format json`)

## [0.11.0] — 2026-08-10

### Added

- **M8 — Audit Log**
  - HMAC-SHA256 hash chain for tamper-evident logging
  - Audit entry types: item CRUD, device admit/revoke, vault unlock/lock, sync events
  - Chain verification: detect tampering, truncation, or reordering
  - Audit log stored inside the encrypted vault file

## [0.10.0] — 2026-08-06

### Added

- **M7 — Import / Export**
  - Bitwarden JSON import (login, secure note, card, identity items)
  - Generic CSV import (flexible header detection)
  - `.zvault-export` encrypted export/import (same crypto as vault files)
  - Plaintext JSON and CSV export (for migration)
  - Sensitive intermediate buffers zeroed during import

## [0.9.0] — 2026-07-28

### Added

- **M4 — Nostr Sync**
  - NIP-44 v2 encryption/decryption (ECDH + HKDF + ChaCha20 + HMAC-SHA256)
  - NIP-01 event signing (Schnorr BIP-340 via k256)
  - NIP-59 gift-wrap (triple-wrap with ephemeral keys, timestamp randomisation)
  - Sync engine: full vault sync messages, stale message guard, LWW item merge
  - Lamport clock for causal ordering
  - NIP-44 spec test vector verification

## [0.8.0] — 2026-07-14

### Added

- **M3 — Device Lifecycle**
  - `DeviceIdentity` — secp256k1 keypair generation, pubkey hex
  - `SecureStorage` trait — OS keychain abstraction; `InMemoryStorage` for tests
  - `OrSet<T>` — generic OR-Set CRDT (add-wins semantics)
  - `DeviceManager` — bootstrap, admit, revoke, merge, flush
  - Deterministic OR-Set tokens for vault reconstruction
  - `test-helpers` feature flag for downstream test crates

## [0.7.0] — 2026-07-01

### Added

- **M2 — Vault Data Model**
  - `Vault` CRUD: `add_item`, `update_item`, `delete_item`, `get_item`, `list_items`
  - `Vault::to_json()` / `from_json()` with zeroing plaintext buffers
  - `VaultFile` — create, open, save, rekey operations
  - Atomic write (write-to-tmp then rename)
  - Version counter for conflict detection
  - Order-preserving deletion (`Vec::remove`)

### Security

- Fixed: `save` now uses `encrypt_with_params` (stored params), not `encrypt` (fresh params)
- Fixed: `to_json()` returns `Zeroizing<Vec<u8>>` — plaintext buffers zeroed on drop
- Fixed: `delete_item` uses `Vec::remove` (order-preserving)
- Fixed: `atomic_write` appends `.tmp` to full filename

## [0.6.0] — 2026-06-18

### Added

- **M1 — Core Crypto**
  - `VaultKey` newtype wrapping `Zeroizing<[u8; 32]>`
  - `KdfParams` — salt + m_cost/t_cost/p_cost; binary and JSON serde
  - `derive_key` — Argon2id RFC 9106
  - `encrypt` / `decrypt` — AES-256-GCM with fresh IV per write
  - `encrypt_with_params` — for rekey and testing
  - Header: magic(8) + salt(32) + kdf_params(12) + iv(12) = 64 bytes
  - Full header included as AES-GCM AAD (tamper detection)

## [0.5.0] — 2026-06-04

### Added

- **M0 — Foundation**
  - Cargo workspace (`zvault-core`, `zvault-cli`)
  - GitHub Actions CI (test, clippy, audit, fmt)
  - Dependabot, PR template, issue templates
  - Module stubs with doc comments
  - `DESIGN.md` — full architecture and threat model
  - `CONTRIBUTING.md`
  - Dual license: MIT OR Apache-2.0

[1.0.0]: https://github.com/boto-coder/zvault/compare/v0.12.0...HEAD
[0.12.0]: https://github.com/boto-coder/zvault/compare/v0.11.0...v0.12.0
[0.11.0]: https://github.com/boto-coder/zvault/compare/v0.10.0...v0.11.0
[0.10.0]: https://github.com/boto-coder/zvault/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/boto-coder/zvault/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/boto-coder/zvault/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/boto-coder/zvault/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/boto-coder/zvault/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/boto-coder/zvault/releases/tag/v0.5.0
