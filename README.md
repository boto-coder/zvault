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
| Desktop (macOS / Windows / Linux) | ✅ Tauri v2 |
| Browser extension (Chrome / Firefox / Safari) | ✅ WXT |
| Android | ✅ Kotlin / Jetpack Compose + UniFFI |
| CLI (`zvault`) | ✅ Rust / clap |

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

Integration tests exercise the full sync protocol stack end-to-end (multi-device sync, revocation, NIP-59 gift-wrap) without network I/O. See `crates/zvault-core/tests/two_device_sync.rs`.

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

## Usage

### CLI

```bash
# Create a new vault
zvault init ~/my.zvault

# Add a login
zvault add --path ~/my.zvault

# List items
zvault list --path ~/my.zvault

# Get item (shows password)
zvault get <item-id> --show-password --path ~/my.zvault

# Export to JSON
zvault export --format json --output backup.json --path ~/my.zvault

# Import from Bitwarden
zvault import --format bitwarden --input export.json --path ~/my.zvault
```

The CLI reads the vault password from an interactive prompt by default. Set `ZVAULT_PASSWORD` for non-interactive/scripting usage.

---

## Desktop App

The desktop app is built with **Tauri v2** (Rust backend + React/TypeScript frontend). It provides a full-featured GUI for vault management with:

- Vault create, unlock, lock, and re-key
- Full CRUD on vault items (logins, secure notes, cards, identities)
- Biometric unlock via OS-native APIs (macOS Touch ID, Windows Hello, Linux libsecret)
- Nostr sync configuration and device management
- TOTP code generation with countdown timer
- Auto-fill via accessibility APIs
- Session lock after inactivity timeout

To build and run:

```bash
cd apps/desktop
npm install
npm run tauri dev
```

---

## Browser Extension

The browser extension is built with **WXT** (Web Extension Tools) for cross-browser support:

- **Chrome** (Manifest V3) and **Firefox** supported
- **Safari** support via WXT's build target
- Core cryptography runs in **WebAssembly** (`zvault-core` compiled via `wasm-pack`)
- Auto-fill via content scripts — HTTPS-only, URI matching before injection
- Optional native messaging bridge to the desktop app for keychain access
- Session key held in memory only; vault data encrypted in `browser.storage.local`

To build:

```bash
cd apps/extension
npm install
npm run dev        # development with hot reload
npm run build      # production build
```

---

## Android

The Android app is built with **Kotlin + Jetpack Compose**, using **UniFFI** to generate Kotlin bindings from the Rust `zvault-core` library:

- Native Rust crypto via auto-generated UniFFI bindings (no JNI boilerplate)
- **Android Keystore API** for secure key storage
- **BiometricPrompt** with Keystore biometric-bound key for fingerprint/face unlock
- **AutofillService API** for system-wide credential auto-fill
- **WorkManager** for background Nostr sync
- Material 3 design with Jetpack Compose UI

The native library is built with `cargo ndk` and linked via Gradle.

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

**To report a vulnerability:** open a [GitHub Security Advisory](https://github.com/boto-coder/zvault/security/advisories/new) — do **not** open a public issue.

---

## Development Status

All milestones (M0–M12) are complete. ZVault v1.0 is feature-complete and hardened with fuzz testing and property-based tests.

| Milestone | Description | Status |
|---|---|---|
| M0 | Foundation — repo, CI, workspace scaffold | ✅ Complete |
| M1 | Core crypto — Argon2id KDF, AES-256-GCM | ✅ Complete |
| M2 | Vault data model — CRUD, serialisation, on-disk format | ✅ Complete |
| M3 | Device lifecycle — keypair, admit/revoke, CRDT | ✅ Complete |
| M4 | Nostr sync — NIP-44/59, relay pub/sub, conflict resolution | ✅ Complete |
| M5 | Desktop app shell — Tauri, React UI, vault CRUD | ✅ Complete |
| M6 | Biometric unlock — keychain, OS secure enclave | ✅ Complete |
| M7 | Import / Export — Bitwarden, CSV, `.zvault-export` | ✅ Complete |
| M8 | Audit log — hash chain, storage, verification | ✅ Complete |
| M9 | Browser extension — WXT, WASM core, auto-fill | ✅ Complete |
| M10 | Android app — UniFFI, Compose UI, Keystore | ✅ Complete |
| M11 | CLI tool — clap subcommands, scripting support | ✅ Complete |
| M12 | Hardening & v1.0 — fuzz, proptest, docs, release | ✅ Complete |

See [DESIGN.md](./DESIGN.md) for the full architecture and threat model, and [CHANGELOG.md](./CHANGELOG.md) for release history.

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT License ([LICENSE-MIT](LICENSE-MIT))

at your option.
