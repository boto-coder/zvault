# ZVault — Development Plan Steering

## Current status

Design phase complete. Development not yet started.

## Milestone overview

| Milestone | Name | Status |
|---|---|---|
| M0 | Foundation — repo, CI, workspace scaffold | 🔲 Not started |
| M1 | Core crypto — Argon2id KDF, AES-256-GCM | 🔲 Not started |
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

## Current milestone: M0

Next actions:
1. Initialise Cargo workspace (`zvault-core`, `zvault-cli` crates)
2. Set up GitHub Actions CI pipeline
3. Add Dependabot and pin all dependencies
4. Write CONTRIBUTING.md, branch protection rules, PR template
