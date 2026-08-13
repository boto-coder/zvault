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

---

## M2 Design Decisions & Gotchas

### VaultFile stores KdfParams — why

`VaultFile` holds `kdf_params: KdfParams` (parsed from the file at open/create
time) in addition to `path`.  This is essential for `save` correctness:

- `save` calls `encrypt_with_params(key, json, &self.kdf_params)` — same salt,
  fresh IV.  The in-memory `VaultKey` stays valid across multiple saves because
  the salt (and therefore the key derivation) does not change.
- If `save` called `encrypt()` instead, a new random salt would be written to
  the file on every save.  The next `open` would derive a different key from
  the new salt, causing a GCM authentication failure even with the correct
  password — the file would appear corrupt.

**Rule:** every `VaultFile` write that does not change the password must use
`encrypt_with_params` with the stored `kdf_params`.  Only `rekey` (which takes
both old and new passwords) is allowed to generate new `KdfParams`.

### Session key pattern

The canonical session flow is:

```
open(password, path) → (VaultFile, VaultKey, Vault)
    ↓ user works with vault
save(&vf, &key, &vault)   // may be called many times; key stays valid
    ↓ user wants to lock
drop(key)                  // Zeroizing<[u8; 32]> zeroed on drop
```

For biometric unlock (M6), the `VaultKey` is wrapped in the OS enclave key and
stored in secure storage.  On biometric unlock, the OS decrypts the wrapped key
and the session resumes from the `save` step without re-running Argon2id.

### Zeroizing plaintext buffers

All intermediate plaintext `Vec<u8>` buffers (JSON output of `to_json`,
decrypted blob from `decrypt`) must be wrapped in `Zeroizing<Vec<u8>>`.

`Vault::to_json()` returns `Zeroizing<Vec<u8>>` (not a bare `Vec<u8>`) so
callers cannot forget to wrap it.  The `decrypt()` return value is a bare
`Vec<u8>`; callers in `vault_file.rs` are responsible for immediately wrapping
it: `let plaintext: Zeroizing<Vec<u8>> = Zeroizing::new(decrypt(...)?);`

### Why delete_item uses Vec::remove not swap_remove

`swap_remove` is O(1) but non-order-preserving: it replaces the deleted item
with the last item in the Vec.  This causes two problems:

1. **Non-deterministic serialisation:** the same logical vault state produces
   different JSON depending on deletion history, making it harder to detect
   genuine conflicts in M4 sync.
2. **CRDT merge complexity:** if two devices independently delete different
   items, their item lists will have diverged ordering, complicating merge.

`Vec::remove` is O(n) but order-preserving.  For typical vault sizes (< 1000
items) this is negligible.

### atomic_write — append .tmp, do not replace extension

`Path::with_extension("tmp")` replaces the last extension:
`my.zvault` → `my.tmp`.  This is wrong for two reasons:

- It destroys the `.zvault` extension from the temp filename, making it
  ambiguous if the process crashes and a `.tmp` file is found.
- If the path has no extension, or already ends in `.tmp`, the temp file and
  the destination path could collide.

The correct approach appends `.tmp` to the full filename:
`my.zvault` → `my.zvault.tmp`.  Implementation:

```rust
let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
tmp_name.push(".tmp");
let tmp = path.with_file_name(tmp_name);
```

### Timestamp vs version counter for conflict detection

`Vault` serialises `created_at` / `updated_at` as RFC3339 strings.  These
timestamps **must not** be used for conflict detection in M4 — wall clocks
differ across devices and sub-second precision varies.

The `version` field (a `u64` counter incremented on every mutation) is the
authoritative conflict signal.  M4 sync must use `version` for CRDT merge
decisions.  Timestamps are metadata for display only.

### VaultItem::Clone — accepted risk

`VaultItem` derives `Clone` because it is necessary for API usability.  Each
clone is a separate heap allocation; its sensitive fields (`password`,
`totp_secret`, etc.) are zeroed independently by its own `Drop` impl.

**Risk:** a clone that outlives its intended scope holds live credential
material in memory longer than necessary.

**Mitigation:** doc comment on `VaultItem` warns callers; `Drop` impl zeroes
all sensitive fields on release.

**Re-evaluate:** M5 (desktop UI) — before any UI layer clones items into
observable state (e.g. form fields), consider switching sensitive fields to
`Zeroizing<String>` or a custom `SecretString` type.
