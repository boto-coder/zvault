# ZVault — Development Plan Steering

## Current status

M0 and M1 complete. Starting M2.

## Milestone overview

| Milestone | Name | Status |
|---|---|---|
| M0 | Foundation — repo, CI, workspace scaffold | ✅ Done |
| M1 | Core crypto — Argon2id KDF, AES-256-GCM | ✅ Done |
| M2 | Vault data model — CRUD, serialisation, on-disk format | 🔲 Not started |
| M3 | Device lifecycle — keypair, admit/revoke, CRDT | 🔲 Not started |
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

## Current milestone: M2

### M2 scope
- `Vault` CRUD: `add_item`, `update_item`, `delete_item`, `get_item`, `list_items`
- Vault serialisation: JSON (serde) + on-disk encryption via M1 crypto
- `VaultFile` struct: combines `Vault` model + header (magic, KDF params, encrypted payload)
- `VaultFile::create(password, path)` and `VaultFile::open(password, path)` high-level API
- `VaultFile::rekey(old_password, new_password)` — re-encrypt with fresh salt
- Round-trip tests: create → write to tempfile → read back → verify items
- Password mismatch test; corrupt-file test

### M2 next actions
1. Implement CRUD methods on `Vault` (currently stubbed in `vault/mod.rs`)
2. Implement `vault_file.rs` with the on-disk read/write API (uses `crate::crypto`)
3. Write integration tests in `crates/zvault-core/tests/`
4. Update DESIGN.md §16 if any design decisions are made

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
