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


---

## M3 Design Decisions & Gotchas

### Why OR-Set CRDT for the device list

The device list needs to support concurrent updates from multiple devices
(concurrent admit, concurrent revoke).  An OR-Set (Observed-Remove Set with
add-wins semantics) is the simplest CRDT that correctly handles this:

- Each admit tags the device_id with a unique token.
- A revoke removes all currently-observed tokens for that device.
- Merging two replicas: union the `adds` and `removes`, then filter `adds` to
  drop any token present in `removes`.
- Concurrent add + remove → add wins (different token survives).
- Concurrent add on both replicas of the same device_id: both tokens survive in
  `adds`, so the device appears once in `elements()` (deduplicated by the
  `DeviceManager` entries layer).

### Deterministic OR-Set tokens for vault reconstruction

Standard OR-Set adds use fresh random tokens.  For `DeviceManager`, we use the
`device_id` UUID itself as the OR-Set token (deterministic).  This allows the
CRDT state to be rebuilt identically from the flat `Vault::devices` list on
every `from_vault()` call, without storing the token out-of-band.

**Trade-off:** if a device is re-admitted after being revoked (edge case), the
same token would appear again.  This is intentionally not supported — revocation
is permanent in v1.

### SigningKey zeroing

`k256::ecdsa::SigningKey` implements `ZeroizeOnDrop` (verified in `ecdsa-0.16`
source).  The short-lived `SigningKey` created in `DeviceIdentity::generate()`
is zeroed on drop without any extra work.

The `secret_bytes: Zeroizing<Vec<u8>>` extracted from the key is also zeroed
on drop before the function returns.

### Why `AeadOsRng` for key generation (not `rand::thread_rng`)

`k256::ecdsa::SigningKey::random()` requires a `CryptoRng + RngCore` from
`rand_core 0.6`.  The workspace uses `rand_core 0.9` for everything else, which
is a different compiled crate.  Importing `rand_core 0.9` here would cause a
type mismatch with `SigningKey::random`.

The same `aes_gcm::aead::OsRng` workaround used in `crypto/mod.rs` is applied
here: `use aes_gcm::aead::OsRng as AeadOsRng` provides the `rand_core 0.6`
`OsRng` that satisfies `SigningKey::random`.

### DeviceManager::Clone — public data only

`DeviceManager` derives `Clone` to support CRDT test scenarios (cloning to
create a diverged replica).  `DeviceManager` holds:

- `OrSet<Uuid>` — only device_id UUIDs; no secrets.
- `Vec<DeviceEntry>` — public device metadata (pubkey hex, label, timestamps); no secrets.

The device secret key lives exclusively in `SecureStorage`, never in
`DeviceManager`.

### test-helpers feature

`InMemoryStorage` (the test-only `SecureStorage` backend) is gated behind
`#[cfg(any(test, feature = "test-helpers"))]`.  The `test-helpers` feature is
declared in `Cargo.toml` so integration test crates can opt in without
enabling it for production builds.


---

## M4 Design Decisions & Gotchas

### NIP-44 message keys on stack — accepted risk

**Finding (LOW):** The NIP-44 encryption implementation derives per-message
keys (ChaCha20 key, ChaCha20 nonce, HMAC key) on the stack without wrapping
them in `Zeroizing<_>`.

**Rationale:** These keys are:
1. Fresh per message — derived from HKDF-expand with a unique nonce each time.
2. Short-lived — they exist only for the duration of a single `nip44_encrypt`
   or `nip44_decrypt` call (microseconds).
3. Not reusable — knowing a message key does not help derive other message keys
   or the conversation key.

Wrapping every intermediate 32-byte array in `Zeroizing` would add complexity
with minimal security benefit given the ephemeral nature of these values.

**Re-evaluate:** if the NIP-44 code is ever refactored to hold message keys
across function boundaries (e.g. streaming encryption), they must be wrapped.

---

## M7 Design Decisions & Gotchas

### CSV crate internal buffers — accepted risk

**Finding (LOW):** The `csv` crate used for LastPass/generic CSV import
maintains internal read buffers that may hold credential data (passwords,
TOTP secrets) after parsing completes.  These buffers are not zeroed because
they are owned by the `csv::Reader` and inaccessible to user code.

**Rationale:**
1. The CSV reader is dropped immediately after parsing — Rust's allocator will
   reclaim the memory, but it is not guaranteed to be zeroed.
2. The import path is a one-shot operation: the user imports once and the
   process context (CLI) or Tauri command returns.  The window of exposure is
   seconds at most.
3. There is no API in the `csv` crate to force-zero internal buffers.

**Mitigation:** Import operations use a scoped block so the `csv::Reader` is
dropped as early as possible.  The imported `VaultItem` values are immediately
encrypted into the vault file.

**Re-evaluate:** if a `csv` crate version exposes a `clear()` or custom
allocator hook, use it.

---

## M8 Design Decisions & Gotchas

### Audit log tail truncation — accepted risk

**Finding (LOW):** When the audit log exceeds the configured maximum entry
count, older entries are truncated from the head of the log.  The truncation
point breaks the hash chain: the first remaining entry's `prev_hash` refers
to a truncated entry that is no longer available for verification.

**Rationale:**
1. Unbounded growth of the audit log is a DoS vector on disk-constrained
   devices (Android, browser extension storage).
2. The `verify_chain()` function accepts a `partial: bool` flag — when `true`,
   it starts verification from the first available entry and reports the chain
   as valid from that point forward.
3. Full verification (from genesis) is available only if the log has never been
   truncated.  This is documented in the `AuditLog::verify` return type which
   distinguishes `FullyVerified` from `PartiallyVerified { from_index }`.

**Mitigation:** The default max entry count (10,000) is generous for typical
usage.  Users are warned in the UI when viewing a partially-verified log.

**Re-evaluate:** if archival of old entries to a separate signed file is
implemented (v2 feature).

---

## M11 Design Decisions & Gotchas

### Password prompt_line not zeroed — accepted risk

**Finding (LOW):** The `rpassword` crate used for interactive password input
returns a `String`.  The CLI wraps this in `Zeroizing<String>` immediately,
but `rpassword` internally allocates a buffer for the prompt line that is not
zeroed after use.

**Rationale:**
1. The prompt line contains only the prompt text ("Enter vault password: "),
   not the password itself.  The password characters are read into a separate
   buffer that `rpassword` zeroes internally (verified in rpassword 5.x source).
2. The exposure is the `String` returned by `rpassword::prompt_password` before
   it is moved into `Zeroizing<String>` — this is a single stack frame with
   no intervening allocations.

**Mitigation:** The CLI does `let password = Zeroizing::new(rpassword::prompt_password(...)?);`
— the `String` is moved (not copied) into `Zeroizing`, so no duplicate exists.

**Re-evaluate:** M12 — verify that the `rpassword` version in use zeroes its
internal buffer.  (Verified: rpassword 5.x uses `zeroize` internally.)

---

## Integration Test Architecture

### `two_device_sync.rs`

Location: `crates/zvault-core/tests/two_device_sync.rs`

This integration test file exercises the complete sync protocol stack
end-to-end, simulating real multi-device scenarios without network I/O.
It is the primary correctness test for the sync and Nostr modules working
together.

**Test cases:**

1. **`full_two_device_sync_cycle`** — The canonical happy path:
   - Device A creates a vault, adds an item
   - Device A admits Device B to the device list
   - Device A builds and sends a full sync message to B
   - Device B receives and applies the sync; verifies the item arrived
   - Device A updates the item (password rotation)
   - Device A sends a second sync to B
   - Device B verifies the updated password
   - A stale replay of the first message is correctly ignored (stale guard)

2. **`revoked_device_sync_rejected`** — Security boundary test:
   - Device A creates vault, admits B
   - Device B builds a sync message containing a malicious item
   - Device A revokes B
   - Device A receives B's sync message → rejected (sender not in live devices)

3. **`gift_wrap_sync_message_end_to_end`** — NIP-59 protocol layer:
   - Verifies that a sync payload can be gift-wrapped and unwrapped
   - Confirms the outer event uses an ephemeral key (not the sender's real key)

4. **`three_device_sync_b_adds_item_invites_c`** — Mesh convergence:
   - A creates vault + item, admits B, syncs to B
   - B adds its own item, admits C, syncs to C
   - C receives ALL items (from both A and B)
   - B syncs back to A → A converges with full state

5. **`full_nostr_protocol_sync_with_gift_wrap`** — Complete protocol stack:
   - Exercises NIP-44 encrypt → NIP-01 sign → NIP-59 gift-wrap → relay delivery
     → unwrap → decrypt → merge
   - Verifies metadata hiding: relay sees only ciphertext + ephemeral key
   - Confirms credentials (passwords, TOTP secrets) are never visible in the
     outer event

**Design rationale:** These tests use `InMemoryStorage` (via the `test-helpers`
feature) and operate entirely in-memory.  No filesystem, no network, no relay.
This makes them fast (< 1s total) and deterministic.  The `create_device`
helper generates a full device identity (secp256k1 keypair, UUID, label) for
each simulated device.
