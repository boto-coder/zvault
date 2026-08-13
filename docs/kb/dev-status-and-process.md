# ZVault — Development Status and Process

## Milestone status

| Milestone | Name | Status |
|---|---|---|
| M0 | Foundation — repo, CI, workspace scaffold | ✅ Done |
| M1 | Core crypto — Argon2id KDF, AES-256-GCM | ✅ Done |
| M2 | Vault data model — CRUD, serialisation, on-disk format | ✅ Done |
| M3 | Device lifecycle — keypair, admit/revoke, CRDT | ✅ Done |
| M4 | Nostr sync — NIP-44/59, relay pub/sub, conflict resolution | 🔲 Not started |
| M5 | Desktop app shell — Tauri, React UI, vault CRUD | 🔲 Not started |
| M6 | Biometric unlock — keychain, OS secure enclave integration | 🔲 Not started |
| M7 | Import / Export — Bitwarden, 1Password, CSV, KDBX | 🔲 Not started |
| M8 | Audit log — hash chain, storage, verification, UI | 🔲 Not started |
| M9 | Browser extension — WXT, WASM core, auto-fill | 🔲 Not started |
| M10 | Android app — UniFFI, Compose UI, Keystore, AutofillService | 🔲 Not started |
| M11 | CLI tool — clap subcommands, scripting support | 🔲 Not started |
| M12 | Hardening & v1.0 — pentest, fuzz, docs, release artefacts | 🔲 Not started |

## Mandatory workflow (process.md)
Every milestone MUST follow this order — no skipping:

```
implement → security-review → fix-all-findings → verify → commit → push
```

### Verify step (all must pass before commit)
```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all
```

### Security review severity levels
- CRITICAL: exploitable, data loss or key compromise possible
- MEDIUM: functional security bug, plaintext not zeroed, API contract violated
- LOW: best-practice violation, footgun
- INFORMATIONAL: worth documenting, no action required

CRITICAL and MEDIUM must be fixed before commit. LOW must be fixed or
explicitly accepted with written rationale in tech.md.

### Commit message format
```
feat(<scope>): implement Mx — <short description>

<body>

Security review findings addressed:
- MEDIUM: <title> — <fix summary>
- LOW: <title> — <fix summary>
```

## Test count by milestone
- M1 crypto: 21 tests
- M2 vault: 13 CRUD + 15 VaultFile = 28 tests
- M3 device: 27 tests
- Total: 82 passing + 1 ignored (slow Argon2id test)

## Key accepted risks
| Milestone | Severity | Finding | Re-evaluate |
|---|---|---|---|
| M2 | LOW | VaultItem::Clone copies sensitive fields | M5 (UI layer) |
| M2 | INFO | Timestamps not used for conflict detection | M4 (sync design) |

## Crypto crate version conflict gotcha
- Workspace uses rand_core 0.9
- aes-gcm 0.10 depends on rand_core 0.6 internally
- These are two different compiled crates — traits don't unify
- Solution: use `aes_gcm::aead::OsRng` (rand_core 0.6) for ALL crypto RNG
- This applies to both crypto/mod.rs AND device/mod.rs (SigningKey::random)
- Import: `use aes_gcm::aead::OsRng as AeadOsRng;`
- RngCore trait: `use aes_gcm::aead::rand_core::RngCore as _;`
