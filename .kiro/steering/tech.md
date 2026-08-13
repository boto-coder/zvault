# ZVault — Tech Stack Steering

## Core library (`zvault-core`)

- **Language:** Rust (safe, memory-efficient, compiles to WASM and native)
- **Vault encryption:** Argon2id (KDF) + AES-256-GCM (at rest)
- **Sync encryption:** NIP-44 (XChaCha20-Poly1305 over ECDH/secp256k1)
- **Key zeroing:** `zeroize` crate on all sensitive types
- **Serialisation:** `serde` + JSON (human-readable) and MessagePack (wire)
- **TOTP:** RFC 6238 via `totp-rs`
- **Crypto crates:** `ring`, `argon2`, `aes-gcm`, `k256`

## Desktop app

- **Shell:** Tauri v2 (Rust backend, native WebView frontend)
- **UI:** React + TypeScript
- **Secure storage:** `keyring` crate → OS keychain (macOS Keychain, Windows Credential Manager, libsecret)
- **Biometric unlock:** OS-native via Tauri plugin (macOS `SecAccessControl`, Windows Hello, libsecret)

## Android app

- **Language:** Kotlin + Jetpack Compose
- **Core bridge:** UniFFI (auto-generated Kotlin bindings from Rust)
- **Secure storage:** Android Keystore API
- **Biometric unlock:** `BiometricPrompt` + Keystore biometric-bound key
- **Auto-fill:** Android AutofillService API
- **Background sync:** WorkManager

## Browser extension

- **Framework:** WXT (multi-browser: Chrome MV3, Firefox, Safari)
- **Language:** TypeScript + React
- **Core crypto:** `zvault-core` compiled to WebAssembly via `wasm-pack`
- **Secure storage:** `browser.storage.local` (encrypted); session key in memory
- **Auto-fill:** content scripts; HTTPS-only; URI match before inject
- **Native messaging:** optional bridge to desktop app for keychain access

## CLI tool (`zvault-cli`)

- **Language:** Rust (thin wrapper over `zvault-core`)
- **Arg parsing:** `clap`
- **Password input:** interactive prompt or `ZVAULT_PASSWORD` env var

## Transport

- **Protocol:** Nostr (NIP-01, NIP-44, NIP-59)
- **Relay comms:** WebSocket over WSS (TLS)
- **Event kinds:** custom / replaceable kinds; gift-wrap (NIP-59) for metadata hiding

## Tooling

- **CI:** GitHub Actions — `cargo test`, `cargo clippy`, `cargo audit`, `cargo fmt`
- **Dependency audit:** `cargo audit` + Dependabot
- **Fuzz testing:** `cargo-fuzz` on vault parser, import parsers, Nostr handler
- **Android CI:** Gradle + GitHub Actions
- **Extension CI:** WXT build + web-ext lint
