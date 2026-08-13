# ZVault — Tech Stack Steering

## Core library (`zvault-core`)

- **Language:** Rust (safe, memory-efficient, compiles to WASM and native)
- **Vault encryption:** Argon2id (KDF) + AES-256-GCM (at rest)
- **Sync encryption:** NIP-44 (XChaCha20-Poly1305 over ECDH/secp256k1)
- **Key zeroing:** `zeroize` crate on all sensitive types
- **Serialisation:** `serde` + JSON (human-readable) and MessagePack (wire)
- **TOTP:** RFC 6238 via `totp-rs`
- **Crypto crates:** `argon2`, `aes-gcm`, `k256` (not `ring` — see M1 decisions)

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

---

## M1 Design Decisions & Gotchas

### Why `argon2` + `aes-gcm` instead of `ring`

`ring` does not expose Argon2id for key derivation — it offers PBKDF2 only.
We need Argon2id (RFC 9106) as the KDF to be resistant to GPU/ASIC attacks.
Using `argon2` (RustCrypto) for KDF and `aes-gcm` (RustCrypto) for AEAD keeps
the entire crypto stack within a single well-audited crate family.

### Why AES-256-GCM over ChaCha20-Poly1305 for vault at rest

AES-256-GCM is FIPS 140-3 approved and benefits from AES-NI hardware
acceleration on every x86-64 target we ship to. This matters for performance
when encrypting/decrypting multi-megabyte vault files.  ChaCha20-Poly1305 is
used for Nostr transport (NIP-44) which is a separate concern.

### Why KDF params are stored in the header

Storing `m_cost`, `t_cost`, `p_cost`, and the salt in the file header allows:
1. **Forward-upgradability:** future vault re-key operations can increase cost
   params without breaking existing files.
2. **No out-of-band state:** a vault file is self-contained; the user only needs
   their password to open it.

### Why KDF params are part of the AES-GCM AAD

The entire header (magic + salt + m/t/p_cost + IV) is passed as Additional
Authenticated Data to AES-GCM.  This means the GCM authentication tag covers
both the ciphertext and the header, so any tampering with the header (e.g.
downgrading cost params to speed up brute-force) is detected and the file
is rejected before decryption.

### On-disk header layout

```
Offset  Size  Field
0       8     Magic bytes: "ZVAULT01"
8       32    Argon2id salt (random per write)
40      4     m_cost (KiB, little-endian u32)
44      4     t_cost (iterations, little-endian u32)
48      4     p_cost (parallelism, little-endian u32)
52      12    AES-GCM nonce / IV (random per write)
64      N     Ciphertext
64+N    16    AES-GCM authentication tag
```

Total fixed overhead per file: **80 bytes** (64-byte header + 16-byte tag).

### rand_core version conflict — `OsRng` gotcha

**Problem:** The workspace pins `rand_core = "0.9"` for `rand 0.9` compatibility.
`aes-gcm 0.10` depends on `rand_core 0.6` (via `aead 0.5` / `crypto-common 0.1`).
These are two different compiled crates; Rust treats them as entirely separate.

Attempting to use `rand_core::OsRng` (0.9) with `Aes256Gcm::generate_nonce()`
or `aes_gcm::aead::OsRng` fails because the `CryptoRng`/`RngCore` traits from
the two crate versions do not unify.

**Solution:** Only use `aes_gcm::aead::OsRng` (which is `rand_core 0.6`'s `OsRng`)
for all random generation inside `crypto/mod.rs` — both for the Argon2 salt
(`OsRng.fill_bytes(...)`) and the AES-GCM nonce (`Aes256Gcm::generate_nonce(&mut AeadOsRng)`).
Do not import `rand_core` directly in the crypto module.

The `rand_core 0.6` `RngCore` trait is brought into scope via
`use aes_gcm::aead::rand_core::RngCore as _;` so that `fill_bytes` is available
without polluting the namespace.

**Longer-term fix:** when `aes-gcm` upgrades to `aead 0.6` (which uses
`rand_core 0.9`), the aliasing workaround can be removed.

### Argon2id default parameters

- `m_cost` = 65536 KiB (64 MiB) — matches OWASP and RFC 9106 "interactive login" baseline
- `t_cost` = 3 — three passes over memory
- `p_cost` = 4 — four parallel threads

These are intentionally kept as constants (`DEFAULT_M_COST` etc.) rather than
hard-coded, so `encrypt_with_params` can accept lower values in tests without
changing production defaults.  Test code uses `m_cost=8, t_cost=1, p_cost=1`
to keep the test suite fast.

### VaultKey: why a newtype over Zeroizing<[u8; 32]>

`Zeroizing<T>` guarantees the memory is zeroed when the value is dropped.
Wrapping it in `VaultKey` prevents the raw bytes from being accidentally passed
to non-crypto code (type safety), and provides a place to add additional
behaviour (e.g. comparison-time equality) in future without changing callers.

The type intentionally does not implement `Clone` or `Copy` to prevent
accidental duplication of key material.
