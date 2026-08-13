# ZVault

**Local-first, end-to-end encrypted password manager with Nostr sync.**

[![CI](https://github.com/boto-coder/zvault/actions/workflows/ci.yml/badge.svg)](https://github.com/boto-coder/zvault/actions/workflows/ci.yml)

ZVault is a privacy-focused secrets manager inspired by Bitwarden. It stores your vault as an encrypted file on each device and uses the [Nostr](https://nostr.com/) protocol as a permissionless, decentralised transport layer for sync. No server is required. No account needed. Nostr relays see only ciphertext — your credentials never leave your devices in plaintext.

---

## Features

- **Argon2id + AES-256-GCM** vault encryption at rest — memory-hard KDF, hardware-accelerated AEAD
- **Nostr sync** (NIP-44 / NIP-59) — decentralised, relay-agnostic, zero-knowledge transport
- **Biometric unlock** — Touch ID, Face ID, Windows Hello, Android BiometricPrompt; never weakens vault encryption
- **Multi-device with explicit trust** — every device is explicitly admitted; revocation is immediate and forward-looking
- **Import / export** — Bitwarden JSON, 1Password 1PUX, LastPass CSV, KeePass KDBX, generic CSV, `.zvault-export` encrypted backup
- **Tamper-evident audit log** — HMAC-SHA256 hash chain; any tampering is detectable
- **TOTP generation** — RFC 6238 via `totp-rs`; secrets stored encrypted in vault
- **Auto-fill** — desktop (accessibility API) and browser extension (content scripts, HTTPS-only, URI match)
- **Clipboard clear** — configurable timeout (default 30 s)
- **No telemetry, no analytics** — the only outbound connections are to user-configured Nostr relays

---

## Platforms

| Platform | Status |
|---|---|
| Desktop (macOS / Windows / Linux) | Tauri v2 — Phase 2 |
| Browser extension (Chrome / Firefox / Safari) | WXT — Phase 3 |
| Android | Kotlin / Jetpack Compose + UniFFI — Phase 3 |
| CLI (`zvault`) | Rust / clap — Phase 3 |

---

## Getting Started

### Prerequisites

- Rust 1.75+ stable (`rustup install stable`)
- `cargo-audit` for dependency security scanning (`cargo install cargo-audit`)

### Build

```bash
git clone https://github.com/boto-coder/zvault.git
cd zvault
cargo build --workspace
```

### Test

```bash
cargo test --workspace --all-features
```

### Lint / format

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

### Dependency audit

```bash
cargo audit
```

---

## Project Structure

```
zvault/
├── crates/
│   ├── zvault-core/      # Core library — crypto, data model, sync engine
│   │   └── src/
│   │       ├── crypto/   # Argon2id KDF, AES-256-GCM
│   │       ├── vault/    # Data model, serialisation, CRUD
│   │       ├── device/   # Device keypair, admit/revoke, OR-Set CRDT
│   │       ├── nostr/    # NIP-01/44/59 event construction
│   │       ├── sync/     # Sync engine, Lamport clock, conflict resolution
│   │       └── audit/    # Audit log, HMAC hash chain
│   └── zvault-cli/       # CLI binary (clap subcommands)
├── apps/
│   ├── desktop/          # Tauri v2 desktop app
│   ├── android/          # Kotlin / Jetpack Compose Android app
│   └── extension/        # WXT browser extension
├── bindings/
│   └── uniffi/           # UniFFI bindings for Android / iOS
├── .github/
│   └── workflows/ci.yml  # CI pipeline (test, audit, coverage)
└── DESIGN.md             # Full architecture and threat model
```

---

## Security

See [DESIGN.md](./DESIGN.md) for the full threat model, cryptographic design, and security checklist.

**To report a vulnerability:** email security@zvault.app — do **not** open a public issue.

---

## Development Status

Currently in **M0 — Foundation**. Core library milestones (M1–M4) are next. See [DESIGN.md §16](./DESIGN.md) for the full milestone plan.

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
