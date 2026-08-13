# ZVault — Development Plan Steering

## Current status

M0, M1, M2, and M3 complete. Starting M4.

## Milestone overview

| Milestone | Name | Status |
|---|---|---|
| M0 | Foundation — repo, CI, workspace scaffold | ✅ Done |
| M1 | Core crypto — Argon2id KDF, AES-256-GCM | ✅ Done |
| M2 | Vault data model — CRUD, serialisation, on-disk format | ✅ Done |
| M3 | Device lifecycle — keypair, admit/revoke, CRDT | ✅ Done |
| M4 | Nostr sync — NIP-44/59, relay pub/sub, conflict resolution | 🔲 Not started |
| M5 | Desktop app shell — Tauri, React UI, vault CRUD | 🔲 Not started |
| M6 | Biometric unlock — keychain, OS secure enclave integration | 🔲 Not started |
| M7 | Import / Export — Bitwarden, 1Password, CSV, KDBX, `.zvault-export` | 🔲 Not started |
| M8 | Audit log — hash chain, storage, verification, UI | 🔲 Not started |
| M9 | Browser extension — WXT, WASM core, auto-fill | 🔲 Not started |
| M10 | Android app — UniFFI, Compose UI, Keystore, AutofillService | 🔲 Not started |
| M11 | CLI tool — clap subcommands, scripting support | 🔲 Not started |
| M12 | Hardening & v1.0 — pentest, fuzz, docs, release artefacts | 🔲 Not started |

## Phase summary

- **Phase 1 (M0–M4):** Core library — ~10–14 weeks
- **Phase 2 (M5–M8):** Desktop app — ~8–10 weeks
- **Phase 3 (M9–M11):** Additional clients — ~10–14 weeks
- **Phase 4 (M12):** Hardening & release — ~4 weeks

**Total estimated duration:** 32–36 weeks (single engineer full-time)

## Definition of done per milestone

1. All features for the milestone are implemented
2. `cargo test --workspace` passes
3. No `clippy` warnings
4. `cargo audit` reports no vulnerabilities
5. Relevant integration tests pass
6. PR reviewed and merged to `main`
7. DESIGN.md updated to reflect any design decisions made during implementation

## Guiding principles

- Core-first: `zvault-core` complete before any UI is built on top
- Desktop-first UI: Tauri desktop is the primary v1 client
- Vertical slices: each phase ships a working end-to-end slice
- Security review at each milestone before proceeding

## Current milestone: M4 — Nostr sync

### M4 scope
- Nostr keypair generation and event signing (NIP-01)
- NIP-44 encryption (XChaCha20-Poly1305) for vault sync events
- NIP-59 gift-wrap for metadata hiding
- WebSocket relay connection (publish + subscribe)
- Sync engine: message construction, conflict resolution using `version` counter
- Integration tests with a local relay or mock

## Completed milestones

### M0 — Foundation (complete)
- Cargo workspace (`zvault-core`, `zvault-cli`)
- GitHub Actions CI (`cargo test`, `cargo clippy`, `cargo audit`, `cargo fmt`)
- Dependabot, PR template, issue templates
- All module stubs with doc comments and `todo!("Mx")` markers
- DESIGN.md full architecture and threat model
- CONTRIBUTING.md, branch protection rules

### M1 — Core Crypto (complete, commit 6075ad1)
Delivered:
- `VaultKey` newtype wrapping `Zeroizing<[u8; 32]>` — key material zeroed on drop
- `KdfParams` struct: salt (32 bytes) + `m_cost`/`t_cost`/`p_cost`; binary and JSON serde
- `derive_key(password, &KdfParams) -> Result<VaultKey>` — Argon2id RFC 9106
- `encrypt(key, plaintext) -> Result<Vec<u8>>` — AES-256-GCM, fresh 12-byte IV per write
- `encrypt_with_params(key, plaintext, &KdfParams) -> Result<Vec<u8>>` — for rekey / testing
- `decrypt(key, blob) -> Result<Vec<u8>>` — authenticates header + ciphertext via GCM tag
- 21 tests: all pass, 0 clippy warnings

On-disk format (64-byte header):
```
magic(8) | salt(32) | m_cost_le(4) | t_cost_le(4) | p_cost_le(4) | iv(12) | ct | tag(16)
```
The full header (magic + KDF params + IV) is included in AES-GCM AAD, so any tampering with the header is detected during authentication.

### M2 — Vault Data Model (complete, commit b054e7e + security fixes)
Delivered:
- `Vault` CRUD: `add_item`, `update_item`, `delete_item` (order-preserving `Vec::remove`), `get_item`, `list_items`
- `Vault::to_json() -> Result<Zeroizing<Vec<u8>>>` and `from_json` — plaintext zeroed on drop
- `Error::ItemNotFound(Uuid)` variant — clean separation from `InvalidVaultFile`
- `VaultFile` struct holding `path` + `kdf_params` (essential for `save` correctness)
- `VaultFile::create(password, path) -> (VaultFile, VaultKey)` — fresh KdfParams, atomic write
- `VaultFile::open(password, path) -> (VaultFile, VaultKey, Vault)` — two-step parse→derive→decrypt
- `VaultFile::save(&key, &vault)` — uses stored `kdf_params` so in-memory key stays valid
- `VaultFile::rekey(old_pw, new_pw) -> (VaultFile, VaultKey, Vault)` — fresh KdfParams, atomic write
- `atomic_write`: appends `.tmp` to full filename (not extension replacement)
- All intermediate plaintext buffers wrapped in `Zeroizing<Vec<u8>>`
- 13 CRUD unit tests + 15 VaultFile integration tests (tempfile)

Security review findings addressed:
- MEDIUM: `save` used `encrypt()` generating new KDF params — fixed to `encrypt_with_params` with stored params
- MEDIUM: plaintext JSON buffers not zeroed — `to_json` now returns `Zeroizing<Vec<u8>>`; `decrypt` output wrapped at call sites
- MEDIUM: `delete_item` used `swap_remove` — fixed to `Vec::remove` (order-preserving)
- LOW: `atomic_write` used `with_extension("tmp")` — fixed to append `.tmp` to full filename
- LOW: `VaultItem::Clone` copies sensitive fields — accepted risk; doc warning added; re-evaluate at M5
- INFO: Timestamp vs version counter — documented in tech.md; M4 must use `version` for conflict detection

### M3 — Device Lifecycle (complete)
Delivered:
- `DeviceIdentity` — in-memory view of this device: `device_id` (UUID) + `pubkey_hex`; secret key never stored in struct
- `DeviceIdentity::generate(label, storage)` — generates secp256k1 keypair, persists secret to `SecureStorage`, returns identity + key material
- `DeviceIdentity::load_secret_key(storage)` — retrieves secret key wrapped in `Zeroizing<Vec<u8>>`
- `SecureStorage` trait — abstraction over OS secure key storage; `InMemoryStorage` for tests (gated behind `#[cfg(any(test, feature = "test-helpers"))]`)
- `OrSet<T>` — generic OR-Set CRDT: `add`, `remove`, `contains`, `elements`, `merge`; add-wins semantics
- `DeviceManager` — wraps vault's device list with CRDT-backed operations: `bootstrap`, `admit`, `revoke`, `merge`, `flush`
- `DeviceManager::from_vault(vault)` — reconstructs CRDT state from `Vault::devices` using deterministic tokens
- `DeviceManager::flush(vault)` — writes device list back to vault, bumps `vault.version`
- `Cargo.toml`: `test-helpers` feature declared so downstream crates can use `InMemoryStorage`
- 27 device module tests (keypair, OR-Set CRDT, DeviceManager lifecycle, CRDT merge, vault round-trips); 82 total workspace tests pass

Security review findings addressed:
- INFO: `k256::ecdsa::SigningKey` implements `ZeroizeOnDrop` — secret scalar zeroed on drop automatically
- INFO: `DeviceManager::Clone` only clones `OrSet<Uuid>` and `Vec<DeviceEntry>` — no secret material
- INFO: `InMemoryStorage` gated to test code; documented as non-production in doc comment