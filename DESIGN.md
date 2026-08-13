# ZVault — Design Document

**Date:** 2026-08-13  
**Status:** Draft v1.0

---

## 1. Overview

ZVault is a local-first, end-to-end encrypted password and secrets manager. It is inspired by Bitwarden but uses the [Nostr](https://nostr.com/) protocol as a permissionless, decentralised transport layer for vault synchronisation across devices. No server is required and no plaintext data ever leaves a device.

Each device holds its own complete copy of the vault. Changes are encrypted and broadcast to trusted peers over Nostr relays. The vault owner controls which devices are authorised to read and write updates.

---

## 2. Core Principles

1. **Local-first** — the vault is a self-contained encrypted file on each device. ZVault works fully offline; Nostr is used only for sync.
2. **Zero-knowledge transport** — Nostr relays see only encrypted ciphertext. They have no access to vault contents, metadata, or device identities in plaintext.
3. **Device-scoped identity** — every device has its own Nostr keypair. This keypair is used only for vault sync messaging, never exposed to the user or shared.
4. **Explicit trust** — a device must be explicitly admitted to the vault by an existing authorised device. No third party can add devices.
5. **Forward-looking revocation** — revoking a device removes it from the authorised set. Future updates will not be encrypted for it. The device cannot decrypt new messages and its own messages will be rejected by other devices.
6. **Defence in depth** — cryptographic protections are layered. A compromise of the Nostr relay, the network, or even the device's Nostr key does not expose vault contents.

---

## 3. Threat Model

| Threat | Mitigation |
|---|---|
| Relay operator reads vault | All vault payloads are AES-256-GCM encrypted; relay sees only ciphertext |
| Network eavesdropper | Same — end-to-end encrypted before hitting the relay |
| Stolen device | Vault file is encrypted with vault master key (Argon2id KDF from master password); without master password it is unreadable |
| Stolen device with biometric unlock enabled | Biometric unlock wraps a session key stored in OS secure enclave (Secure Enclave / Android StrongBox); the master key is never derived from biometrics alone — biometric auth gates access to an OS-held key blob that decrypts the vault key |
| Biometric spoofing | Delegated entirely to the OS biometric stack (Face ID, Touch ID, Android BiometricPrompt); ZVault does not implement its own biometric matching |
| Rogue/removed device tries to inject updates | All updates are signed by sender Nostr key and verified against the authorised device list |
| Removed device reads future updates | Future messages are not encrypted for removed device's public key |
| Relay replays old messages | Each message carries a monotonic `clock` (Lamport-style) and a `vault_version`; stale/replayed messages are ignored |
| Man-in-the-middle on join flow | Out-of-band verification: device pubkeys are verified via QR code or manual fingerprint check before admission |
| Audit log tampering | Each audit log entry is chained via HMAC; any deletion or modification breaks the chain and is detectable on next verification |
| Import of malicious data | All imported items are validated against the VaultItem schema; unknown fields are dropped; no executable content is imported |

---

## 4. Data Model

### 4.1 Vault

```
Vault {
  id:          UUID           // stable identifier, random, never changes
  version:     u64            // monotonically increasing, incremented on every write
  created_at:  timestamp
  updated_at:  timestamp
  items:       VaultItem[]
  devices:     DeviceEntry[]
}
```

### 4.2 VaultItem

```
VaultItem {
  id:          UUID
  kind:        Login | SecureNote | Card | Identity
  name:        String
  folder:      Option<UUID>
  favourite:   bool
  created_at:  timestamp
  updated_at:  timestamp

  // Login-specific
  username:    Option<String>
  password:    Option<String>   // stored encrypted at rest
  totp_secret: Option<String>
  uris:        URI[]

  // SecureNote
  note:        Option<String>

  // Card
  card_number: Option<String>
  expiry:      Option<String>
  cvv:         Option<String>
  cardholder:  Option<String>

  // Identity fields (name, address, phone, email, etc.)
  identity:    Option<IdentityFields>
}
```

### 4.3 BiometricUnlockConfig

Stored per-device in OS secure storage alongside the device keypair. Never written to the vault file or synced.

```
BiometricUnlockConfig {
  enabled:           bool
  // OS-specific key reference — not the key material itself
  key_handle:        String        // Keychain item name / Keystore alias / Credential Manager target
  wrapped_vault_key: [u8]          // AES-256-GCM encrypted vault key, wrapped by the OS-held biometric key
  iv:                [u8; 12]      // IV used for the wrapping encryption
  created_at:        timestamp
  last_used_at:      Option<timestamp>
}
```

The `wrapped_vault_key` is produced once when the user enables biometric unlock:
1. OS generates a biometric-bound key in the secure enclave (never exportable)
2. ZVault uses that OS key to AES-256-GCM encrypt the in-memory vault master key
3. The ciphertext (`wrapped_vault_key`) is stored in OS secure storage
4. On subsequent unlocks: biometric auth → OS releases the enclave key → ZVault decrypts `wrapped_vault_key` → vault master key in memory

### 4.4 DeviceEntry

```
DeviceEntry {
  device_id:    UUID          // random, assigned at first run
  nostr_pubkey: [u8; 32]      // secp256k1 pubkey (hex)
  label:        String        // human-readable name
  added_at:     timestamp
  added_by:     UUID          // device_id that admitted this device
  revoked:      bool
  revoked_at:   Option<timestamp>
  revoked_by:   Option<UUID>
}
```

### 4.5 On-disk vault file

The vault is stored as an opaque encrypted blob:

```
[magic: 8 bytes "ZVAULT01"]
[kdf_params: 64 bytes — Argon2id salt + params]
[encrypted_payload: N bytes — AES-256-GCM(master_key, vault_json)]
[auth_tag: 16 bytes]
```

The `master_key` is derived from the user's master password via Argon2id (memory-hard KDF). The vault JSON is never written in plaintext.

---

## 5. Cryptographic Design

### 5.1 Vault master key

- KDF: **Argon2id** (RFC 9106, high-memory variant)
  - `m_cost` = 64 MiB, `t_cost` = 3, `p_cost` = 4 (adjustable per device capability)
  - Salt: 32 random bytes, stored in vault header
- Encryption: **AES-256-GCM** (256-bit key, 96-bit random IV per write)
- Key never leaves the device's memory in derived form; vault is re-encrypted on every write

### 5.2 Nostr device identity

- Each device generates a **secp256k1** keypair on first run
- The secret key is stored in OS-specific secure storage:
  - Desktop: OS keychain (macOS Keychain, Windows Credential Manager, libsecret on Linux)
  - Android: Android Keystore
  - Browser extension: `browser.storage.local` (encrypted with a key held in session memory)
- The keypair is used **only** for Nostr protocol operations (signing events, NIP-04/NIP-44 encryption)

### 5.3 Vault sync message encryption

Vault sync uses **NIP-44** (XChaCha20-Poly1305 with ECDH shared secret) for point-to-point message encryption.

When device A sends a vault update to device B:
1. A derives shared secret: `ECDH(A_privkey, B_pubkey)` → shared secret
2. A encrypts payload: `XChaCha20-Poly1305(shared_secret, vault_delta)`
3. A wraps in a Nostr `kind: 1059` gift-wrap event and publishes to relays
4. B fetches events, unwraps with its private key, decrypts payload

For multi-device broadcast, A sends one encrypted message per authorised device (same pattern as NIP-17 direct messages).

### 5.4 Sync payload

```
SyncMessage {
  msg_id:       UUID
  sender:       UUID              // device_id
  vault_id:     UUID
  vault_version: u64
  clock:        u64               // Lamport clock
  op:           Full | Delta      // Full = entire vault, Delta = patch set
  payload:      EncryptedBytes    // encrypted vault or CRDT delta
  signature:    [u8; 64]          // Nostr event signature (secp256k1 Schnorr)
}
```

---

## 6. Passphrase-less Unlock (Biometric)

### 6.1 Goals

- Allow users to unlock their vault with Face ID, Touch ID, or fingerprint instead of typing the master password every time.
- Never weaken the cryptographic protection of the vault file itself.
- Never transmit biometric data or biometric-derived keys off-device.
- Remain opt-in; the vault is always fully accessible via the master password regardless of biometric state.

### 6.2 Platform support

| Platform | Mechanism | API |
|---|---|---|
| macOS | Touch ID / Apple Watch | `SecAccessControl` with `biometryAny` flag; key stored in Keychain |
| Windows | Windows Hello (fingerprint / face / PIN) | DPAPI with Windows Hello credential; key stored in Credential Manager |
| Linux desktop | Polkit / GNOME Keyring / KWallet | libsecret with user session unlock; biometric hardware-dependent |
| Android | Fingerprint / Face unlock | `BiometricPrompt` + `KeyStore` with `setUserAuthenticationRequired(true)` |
| iOS (future) | Face ID / Touch ID | `SecAccessControl` with `biometryAny`; key in Secure Enclave |
| Browser extension | Delegates to desktop app via native messaging | Extension itself has no biometric capability |

### 6.3 Unlock flow

```
User taps "Unlock with biometric"
  → OS presents biometric challenge
  → On success: OS releases enclave key
  → ZVault: AES-256-GCM decrypt(wrapped_vault_key, enclave_key) → vault_master_key
  → Vault file decrypted with vault_master_key
  → Session begins; vault_master_key held in memory, zeroed on lock
```

### 6.4 Enabling biometric unlock

Biometric unlock can only be enabled from an already-unlocked session (i.e., user has proven knowledge of master password in the current session):

1. User opens Settings → Security → Enable biometric unlock
2. App confirms master password one more time (anti-clickjacking)
3. OS prompts for biometric enrolment confirmation
4. OS generates a biometric-bound key in secure enclave (never exportable)
5. ZVault encrypts current in-memory vault_master_key with enclave key → `wrapped_vault_key`
6. `wrapped_vault_key` + `key_handle` stored in `BiometricUnlockConfig` in OS secure storage
7. Feature is now active for subsequent unlocks

### 6.5 Disabling / invalidation

- User can explicitly disable biometric unlock in settings (clears `BiometricUnlockConfig`)
- If OS reports biometric data changed (new fingerprint enrolled, Face ID re-configured), the wrapped key is invalidated by the OS — ZVault detects this and falls back to master password, prompting the user to re-enable biometric unlock
- Revoking a device (section 6.4 of device lifecycle) also clears biometric config on that device
- Master password change: ZVault re-wraps the new vault_master_key and updates `wrapped_vault_key`

### 6.6 Security properties

- The vault file remains protected by Argon2id + AES-256-GCM regardless of biometric state
- A stolen encrypted vault file cannot be unlocked without either the master password or access to the specific device's secure enclave
- Biometric unlock is strictly a convenience layer on top of the existing key hierarchy
- `zvault-core` has a `BiometricUnlock` feature flag; platforms that do not support it compile it out

---

## 7. Device Lifecycle

### 6.1 First device (vault creation)

1. User installs ZVault on device A
2. App generates secp256k1 keypair → stored in secure storage
3. User sets master password → Argon2id KDF → vault encryption key
4. Empty vault is created, device A is added to `devices[]` as the first entry
5. User selects preferred Nostr relays (or uses defaults)

### 6.2 Joining: join-by-pubkey flow

1. User installs ZVault on device B (new device)
2. B generates its own keypair → displays its pubkey (hex + QR)
3. User opens existing device A, enters B's pubkey
4. A verifies fingerprint out-of-band (user confirms)
5. A sends a **VaultInvite** message encrypted to B's pubkey via Nostr:
   ```
   VaultInvite {
     vault_id:     UUID
     invited_by:   UUID          // A's device_id
     full_vault:   EncryptedBytes  // vault encrypted to B's pubkey
     device_list:  DeviceEntry[]
   }
   ```
6. B receives invite, decrypts it, prompts user to accept
7. B stores vault locally, adds itself to device list, broadcasts an **Ack**
8. A (and other devices) receive Ack, add B to authorised devices, re-broadcast device list update

### 6.3 Joining: invite link flow

1. Device A generates a **one-time invite token** (random 32 bytes)
2. Token is encoded as a deep link: `zvault://join/<relay_hints>/<vault_id>/<invite_token>`
3. New device B opens link, derives a temporary Nostr keypair from the token (or just uses it as a pre-shared key for initial message)
4. B sends a **JoinRequest** encrypted with the invite token to the relay
5. A listens for join requests on the token channel, admits B as in steps 5–8 above
6. Token is burned after first use

### 6.4 Device revocation

1. Admin device opens device management UI
2. Selects device to revoke → sets `revoked = true` in vault's `devices[]`
3. Increments `vault_version`, re-encrypts vault and broadcasts update to all **remaining** authorised devices (revoked device is excluded from recipient list)
4. Going forward, incoming messages from the revoked device's pubkey are silently dropped by all remaining devices
5. The revoked device still has its local vault copy (frozen at last sync point), but cannot push or pull future changes

**Note:** revocation does not retroactively prevent reading of already-synced data. To fully remove a device's access to historical data would require re-keying the vault (rotating master key), which is supported as an optional "emergency re-key" operation.

### 6.5 Emergency re-key

1. User triggers "Re-key vault" (e.g., after suspected compromise)
2. A new vault encryption key is generated from a new master password (or re-derived with a new salt)
3. All vault items are re-encrypted under the new key
4. A full vault broadcast is sent to all remaining authorised devices
5. Re-keyed vault is incompatible with any revoked/excluded devices

---

## 8. Conflict Resolution

Because devices can edit the vault offline simultaneously, conflicts must be resolved:

- Each vault item carries a `updated_at` timestamp and a `version` counter
- The vault as a whole uses a **last-write-wins (LWW)** strategy at item granularity (sufficient for secrets manager use cases where concurrent edits to the same item are rare)
- For device list operations (admit/revoke), a **state-based CRDT** (OR-Set) is used to ensure convergence: additions are additive; revocations are permanent tombstones that win over additions
- Full vault syncs always win over deltas if `vault_version` of the full sync is higher
- Future: consider Automerge/CRDT for field-level merge on vault items

---

## 9. Platform Architecture

### 8.1 Core library (`zvault-core`)

A platform-agnostic Rust library that implements:
- Vault data model, serialisation (JSON + MessagePack)
- Argon2id KDF + AES-256-GCM vault encryption/decryption
- Nostr keypair generation, event signing, NIP-44 encryption
- Sync engine (message construction, validation, conflict resolution)
- Device management (admit, revoke, list)
- TOTP generation (RFC 6238)

### 8.2 Desktop app (Tauri v2)

- **Frontend:** React + TypeScript
- **Backend shell:** Rust (Tauri commands invoke `zvault-core`)
- **Platforms:** macOS, Windows, Linux
- **Secure storage:** OS keychain via `keyring` crate
- **Auto-fill:** OS accessibility APIs (best-effort)

### 8.3 Android app (Kotlin / Jetpack Compose)

- Core crypto logic via `zvault-core` compiled as JNI (UniFFI bindings)
- UI: Jetpack Compose
- Secure storage: Android Keystore API
- Auto-fill: Android AutofillService API
- Background sync: WorkManager

### 8.4 Browser extensions (WXT framework)

- **Targets:** Chrome (MV3), Firefox (MV3/MV2), Safari (via Xcode)
- **Language:** TypeScript + React (popup UI)
- **Core crypto:** `zvault-core` compiled to WebAssembly (wasm-pack)
- **Secure storage:** `browser.storage.local` (encrypted); session key held in memory
- **Auto-fill:** content scripts inject into login forms
- **Native messaging:** optional bridge to desktop app for keychain access

### 8.5 CLI tool (`zvault-cli`)

- Thin Rust binary wrapping `zvault-core`
- For power users and scripting
- Unlocks vault via master password prompt or env var (CI use cases)

---

## 10. Nostr Relay Strategy

- ZVault publishes to user-configured relays (defaults provided)
- All vault sync events use `kind: 10050` (replaceable) or custom ephemeral kinds to avoid relay storage pollution
- Events are tagged with the vault's ID (hashed/blinded) so devices can filter efficiently
- Gift-wrap (NIP-59) hides the true sender/recipient from relay operators
- Devices poll relays on startup and maintain a persistent WebSocket connection when online
- Relay list itself is stored in the vault (synced across devices)

---

## 11. Project Structure (Monorepo)

```
zvault/
├── crates/
│   ├── zvault-core/          # Core library (Rust)
│   │   ├── src/
│   │   │   ├── vault/        # Data model, serialisation
│   │   │   ├── crypto/       # KDF, AES-GCM, Argon2id
│   │   │   ├── nostr/        # Keypair, event signing, NIP-44
│   │   │   ├── sync/         # Sync engine, conflict resolution
│   │   │   └── device/       # Device lifecycle
│   │   └── Cargo.toml
│   └── zvault-cli/           # CLI tool (Rust)
│       └── Cargo.toml
├── apps/
│   ├── desktop/              # Tauri desktop app
│   │   ├── src-tauri/        # Tauri Rust backend
│   │   └── src/              # React frontend
│   ├── android/              # Kotlin/Compose Android app
│   │   └── app/
│   └── extension/            # WXT browser extension
│       └── src/
├── bindings/
│   └── uniffi/               # UniFFI bindings for Android/iOS
├── .kiro/
│   └── steering/             # Kiro steering files
├── docs/
│   └── DESIGN.md             # This document
├── DESIGN.md                 # Top-level design document (this file)
├── Cargo.toml                # Workspace root
└── README.md
```

---

## 12. Technology Stack

| Layer | Technology | Rationale |
|---|---|---|
| Core crypto + sync | **Rust** | Memory safety, `no_std` portability, best-in-class crypto crates (`ring`, `argon2`, `aes-gcm`) |
| Desktop app shell | **Tauri v2** | Rust backend + web frontend, smaller bundle than Electron, native OS APIs |
| Desktop UI | **React + TypeScript** | Mature ecosystem, good component libraries |
| Android | **Kotlin + Jetpack Compose** | First-class Android support, modern declarative UI |
| Android crypto bridge | **UniFFI** | Auto-generates Kotlin bindings from Rust |
| Browser extensions | **WXT + TypeScript** | Multi-browser build framework (Chrome/Firefox/Safari from one codebase) |
| Extension crypto | **wasm-pack** | Compile `zvault-core` to WebAssembly for in-browser use |
| Transport | **Nostr (NIP-01, NIP-44, NIP-59)** | Permissionless, decentralised, no account needed |
| Vault encryption | **Argon2id + AES-256-GCM** | Industry standard, hardware-accelerated |
| Sync messaging | **NIP-44 (XChaCha20-Poly1305)** | Authenticated encryption over Nostr |

---

## 13. Import / Export

### 13.1 Goals

- Allow users to migrate into ZVault from other password managers without manual re-entry.
- Allow users to export their vault for backup or migration to another tool.
- Never expose plaintext credentials to the filesystem longer than necessary.
- Validate and sanitise all imported data to prevent injection or corruption.

### 13.2 Supported import formats

| Source | Format | Notes |
|---|---|---|
| Bitwarden | JSON (encrypted or unencrypted) | Primary target; schema is well-documented |
| 1Password | 1PUX (zip of JSON) or CSV | 1PUX preferred; CSV as fallback |
| LastPass | CSV | Limited field mapping; notes may lose structure |
| KeePass | KDBX 3.x / 4.x | Via `keepass` crate or XML export |
| Generic CSV | Configurable column mapping | Power-user escape hatch |
| ZVault encrypted export | `.zvault-export` (see §13.4) | Round-trip backup format |

### 13.3 Import flow

1. User selects source format and provides the export file
2. ZVault parses and validates the file in memory (never written to disk in plaintext)
3. Items are mapped to `VaultItem` schema; unknown fields are dropped; no executable content is accepted
4. User is shown a preview: item count by type, any items that failed mapping
5. User confirms; items are merged into the vault (deduplication by name + URI match, user prompted on conflict)
6. Vault is re-encrypted and saved; an audit log entry is written: `import: N items from <format>`
7. The source file is not deleted by ZVault (user's responsibility)

### 13.4 ZVault encrypted export format (`.zvault-export`)

A portable, self-contained encrypted export file:

```
[magic: 8 bytes "ZVEXPORT"]
[version: 2 bytes]
[kdf_params: 64 bytes — Argon2id salt + params]
[encrypted_payload: N bytes — AES-256-GCM(export_key, export_json)]
[auth_tag: 16 bytes]
```

- `export_key` is derived from an **export passphrase** chosen by the user at export time (separate from the master password — this passphrase is what you store alongside the backup file)
- `export_json` contains all vault items in full, plus metadata (vault ID, export timestamp)
- Device-specific data (keypairs, biometric config) is **not** included in exports
- Sync history and audit log are **not** included by default (opt-in flag)

### 13.5 Plaintext export

- Plaintext export (CSV or JSON) is available for interoperability but gated behind an explicit confirmation dialog warning the user
- Plaintext exports are written to the OS temp directory, opened in the app, and the file is securely deleted (overwritten + unlinked) after the user acknowledges
- Auto-fill never targets plaintext export files

### 13.6 Security checklist for import/export

- [ ] Source file parsed entirely in memory; never decrypted to disk
- [ ] Imported strings validated: max field lengths enforced, no executable content
- [ ] Export passphrase never stored; Argon2id KDF params stored in file header
- [ ] Plaintext exports: temp file securely deleted after use
- [ ] Import and export events written to audit log

---

## 14. Audit Log

### 14.1 Goals

- Provide a local, tamper-evident record of all security-relevant events.
- Allow users to review what happened to their vault and detect unexpected access.
- Keep the log on-device only; it is not synced via Nostr (each device has its own log).

### 14.2 Logged events

| Category | Events |
|---|---|
| Vault access | Unlock (success/failure), lock, session timeout |
| Vault mutations | Item created, item updated, item deleted, vault re-keyed |
| Device lifecycle | Device added, device revoked, device renamed |
| Sync | Sync started, sync completed, conflict resolved, rejected message (bad signature / stale clock) |
| Import / Export | Import completed (source, item count), export created (format), plaintext export created |
| Biometric | Biometric unlock enabled, disabled, invalidated by OS |
| Auth | Master password changed, biometric auth success, biometric auth failure |

### 14.3 Log entry schema

```
AuditEntry {
  seq:        u64           // monotonically increasing per device
  timestamp:  timestamp     // UTC
  device_id:  UUID          // which device generated this entry
  event:      EventKind     // enum (see table above)
  detail:     String        // human-readable summary (no plaintext credentials)
  prev_hmac:  [u8; 32]      // HMAC-SHA256 of previous entry (chain link)
  hmac:       [u8; 32]      // HMAC-SHA256(chain_key, seq || timestamp || event || detail || prev_hmac)
}
```

### 14.4 Tamper evidence

The log uses a **hash chain** (similar to a blockchain without consensus):

- Each entry's `hmac` covers its own content plus the `prev_hmac` of the previous entry
- The `chain_key` is derived from the vault master key via HKDF: `HKDF(vault_master_key, "audit_chain_key")`
- Deleting, reordering, or modifying any entry breaks all subsequent HMACs
- ZVault verifies the chain on startup and on demand; any break is surfaced as a warning
- Because the chain key is derived from the vault master key, an attacker who can read the vault file could in theory recompute valid HMACs — but they would need the master password first, which means it's not a weaker guarantee than the vault itself

### 14.5 Storage

- The audit log is stored in a separate file alongside the vault: `<vault_name>.audit`
- The file uses the same AES-256-GCM encryption as the vault file (same master key)
- The log is append-only from the application's perspective; no UI operation deletes log entries
- Log rotation: entries older than 90 days (configurable) are archived to `<vault_name>.audit.archive.<YYYY-MM>` and compressed; the active log keeps the last entry of the archived batch for chain continuity

### 14.6 UI

- Audit log viewer available in Settings → Security → Audit Log
- Filterable by date range, event category, device
- "Verify chain integrity" button triggers a full HMAC chain check
- Export audit log as encrypted `.zvault-export` (audit flag set) or plaintext CSV (same warnings as §13.5)

---

## 15. Security Checklist

- [ ] Master password never stored; only the derived key (held in memory, zeroed on lock)
- [ ] Vault file always encrypted at rest; never written in plaintext
- [ ] Device private keys stored in OS secure storage (Keychain/Keystore)
- [ ] All Nostr messages use gift-wrap (NIP-59) to hide metadata
- [ ] Relay communication over WSS (TLS)
- [ ] Incoming messages validated: signature, sender in authorised list, vault version monotonic
- [ ] Revoked devices excluded from all future encrypted broadcasts
- [ ] TOTP secrets stored as vault items (encrypted at rest)
- [ ] Auto-fill: only inject on HTTPS pages
- [ ] Auto-fill: match URI before injecting (no cross-site leakage)
- [ ] Clipboard: clear after configurable timeout (default 30s)
- [ ] Session lock after inactivity timeout (configurable)
- [ ] Memory: zero sensitive material (passwords, keys) after use using `zeroize`
- [ ] Dependencies: audit via `cargo audit`, Dependabot
- [ ] No telemetry, no analytics, no external calls except user-configured Nostr relays

---

## 16. Development Plan

### 16.1 Guiding principles

- **Core-first:** `zvault-core` must be feature-complete and well-tested before any UI surface is built on top of it.
- **Desktop-first UI:** the Tauri desktop app is the primary v1 client; Android and browser extension follow.
- **Vertical slices:** each phase ships a working end-to-end slice (create vault → unlock → use) rather than all-or-nothing.
- **Security review at each milestone:** crypto and auth code reviewed before building on top of it.

### 16.2 Milestones overview

| Milestone | Name | Deliverable |
|---|---|---|
| M0 | Foundation | Repo, CI, dependency audit, workspace scaffold |
| M1 | Core crypto | Vault encrypt/decrypt, Argon2id KDF, AES-256-GCM |
| M2 | Vault data model | Full CRUD on vault items, serialisation, on-disk format |
| M3 | Device lifecycle | Keypair generation, device admit/revoke, CRDT device list |
| M4 | Nostr sync | NIP-44 message encryption, gift-wrap, relay pub/sub, conflict resolution |
| M5 | Desktop app shell | Tauri app with React, vault create/unlock/lock, item list/edit/delete |
| M6 | Biometric unlock | Keychain integration, BiometricUnlock feature on desktop + Android |
| M7 | Import / Export | Bitwarden JSON, 1Password 1PUX, CSV, `.zvault-export` encrypted backup |
| M8 | Audit log | AuditEntry schema, hash chain, storage, chain verification, UI viewer |
| M9 | Browser extension | WXT extension, WASM core, auto-fill content scripts |
| M10 | Android app | UniFFI bindings, Compose UI, Android Keystore, AutofillService |
| M11 | CLI tool | `zvault-cli` wrapping core; stdin/env password; scripting support |
| M12 | Hardening & release | Penetration test, `cargo audit`, fuzz testing, documentation, v1.0 tag |

### 16.3 Phase detail

#### Phase 1 — Core (M0–M4) — estimated 10–14 weeks

**M0 · Foundation (1 week)**
- Initialise Cargo workspace with `zvault-core`, `zvault-cli` crates
- Set up CI: `cargo test`, `cargo clippy`, `cargo audit`, `cargo fmt` on every PR
- Pin all dependencies; add Dependabot
- Establish `CONTRIBUTING.md`, branch protection, PR template

**M1 · Core crypto (2 weeks)**
- `crypto` module: Argon2id KDF, AES-256-GCM encrypt/decrypt, `zeroize` for key material
- On-disk vault format: magic bytes, KDF params header, encrypted payload, auth tag
- 100% unit test coverage on all crypto paths; property-based tests via `proptest`
- Security review of crypto module before proceeding

**M2 · Vault data model (2 weeks)**
- `vault` module: `Vault`, `VaultItem` (Login, SecureNote, Card, Identity), `DeviceEntry`
- `BiometricUnlockConfig` struct (data only; integration in M6)
- Serialisation: JSON (human-readable) + MessagePack (compact wire format)
- CRUD operations; version increment on write; `updated_at` tracking

**M3 · Device lifecycle (2 weeks)**
- `device` module: keypair generation, secp256k1 via `k256` crate
- Secure storage abstraction (`keyring` crate); platform backends: macOS/Windows/Linux
- Device admit flow (VaultInvite), revoke flow, OR-Set CRDT for device list
- Invite link generation and parsing (`zvault://join/...`)

**M4 · Nostr sync (3 weeks)**
- `nostr` module: NIP-01 event construction and signing, NIP-44 encryption, NIP-59 gift-wrap
- `sync` module: full vault and delta sync, Lamport clock, version validation
- Relay WebSocket client (connect, publish, subscribe, reconnect)
- Conflict resolution: LWW for items, OR-Set for devices
- Integration tests: two-device sync round-trip against a local relay (e.g., `nostr-rs-relay` in Docker)

#### Phase 2 — Desktop App (M5–M8) — estimated 8–10 weeks

**M5 · Desktop app shell (3 weeks)**
- Tauri v2 project scaffold; Tauri commands for all `zvault-core` operations
- React + TypeScript UI: vault create wizard, master password unlock screen, item list, item detail/edit, device management
- OS keychain integration for device keypair
- Session lock / inactivity timeout

**M6 · Biometric unlock (2 weeks)**
- macOS: `SecAccessControl` + Touch ID gated Keychain item
- Windows: DPAPI + Windows Hello
- Linux: libsecret session unlock (best-effort)
- `BiometricUnlockConfig` persistence; enable/disable flow; OS invalidation handling
- Android implementation deferred to M10

**M7 · Import / Export (2 weeks)**
- Import parsers: Bitwarden JSON, 1Password 1PUX, LastPass CSV, KeePass XML, generic CSV
- Schema validation and field mapping; conflict preview UI
- `.zvault-export` encrypted export writer and reader
- Plaintext CSV/JSON export with temp-file secure deletion
- Audit log entries for import/export events

**M8 · Audit log (1 week)**
- `AuditEntry` schema; HMAC-SHA256 hash chain; `chain_key` via HKDF from vault master key
- Append-only log storage (`<vault>.audit`) with same encryption as vault
- Chain verification on startup; warning UI on chain break
- Audit log viewer in Settings: filter by date/event/device, verify chain button
- Log rotation: archive entries older than 90 days

#### Phase 3 — Additional Clients (M9–M11) — estimated 10–14 weeks

**M9 · Browser extension (4 weeks)**
- WXT project scaffold; `wasm-pack` build of `zvault-core` to WASM
- Popup UI: unlock, item search, copy username/password
- Content scripts: auto-fill on login form focus; URI match before inject; HTTPS-only
- Native messaging bridge to desktop app (optional, for keychain delegation)
- Chrome MV3 target first; Firefox port; Safari via Xcode

**M10 · Android app (4 weeks)**
- UniFFI binding generation for `zvault-core`
- Kotlin / Jetpack Compose UI: unlock, item list, item detail, device management
- Android Keystore biometric unlock (completes M6 for Android)
- AutofillService integration
- WorkManager background sync
- APK build via GitHub Actions

**M11 · CLI tool (2 weeks)**
- `zvault-cli`: clap-based subcommands: `unlock`, `lock`, `list`, `get`, `add`, `edit`, `delete`, `devices`, `sync`, `import`, `export`, `audit`
- Master password via interactive prompt or `ZVAULT_PASSWORD` env var (CI)
- Machine-readable output (`--json` flag)
- Shell completion scripts (bash, zsh, fish)

#### Phase 4 — Hardening & Release (M12) — estimated 4 weeks

**M12 · Hardening & v1.0 (4 weeks)**
- Penetration test / security review of full application
- Fuzz testing: vault parser, import parsers, Nostr message handler (`cargo-fuzz`)
- `cargo audit` clean; all dependencies reviewed
- Performance profiling: Argon2id params tuned per platform
- End-to-end tests: full sync flow, import round-trip, biometric unlock flow
- User documentation, README, CHANGELOG
- v1.0 tag and release artefacts (desktop installers, extension store submission, APK)

### 16.4 Estimated timeline

```
Week:  1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36
M0:    ██
M1:       ████
M2:           ████
M3:               ████
M4:                   ██████
M5:                               ██████
M6:                                     ████
M7:                                         ████
M8:                                             ██
M9:                                               ████████
M10:                                                      ████████
M11:                                                              ████
M12:                                                                  ████████
```

Total estimated duration: **32–36 weeks** (one engineer full-time; can be parallelised across multiple engineers at Phase 2/3).

### 16.5 Definition of done per milestone

A milestone is complete when:
1. All features described above are implemented
2. Unit tests pass (`cargo test --workspace`)
3. No `clippy` warnings
4. `cargo audit` reports no vulnerabilities
5. Relevant integration tests pass
6. A PR has been reviewed and merged to `main`
7. DESIGN.md updated to reflect any design decisions made during implementation

---

## 17. Open Questions / Future Work

The following items are **deferred** — they are not required for v1 but are tracked here for future planning.

1. **iOS client** — architecture is the same as Android (UniFFI + Swift), deferred to v2
2. **Field-level CRDT** — Automerge integration for true merge semantics on concurrent edits; v1 uses LWW at item granularity
3. **Emergency re-key UX** — needs careful UX design; currently a power-user feature
4. **Relay discovery** — how do new devices find relays if join happens before relay list is synced? (Proposal: embed relay hints in invite link)
5. **Organisation/shared vaults** — multi-user shared vault with role-based access; requires group encryption key scheme

---

## 18. Lessons Learned / Design Decisions Log

### Why Nostr instead of a custom sync server?

A traditional sync server (like Bitwarden's) requires you to trust the server operator, run infrastructure, and manage accounts. Nostr gives us a permissionless broadcast medium — we only need relays (commodity, interchangeable infrastructure), and end-to-end encryption means relays learn nothing. Users can run their own relay trivially.

### Why per-device Nostr keypairs instead of one shared vault key?

A shared keypair would mean all devices are indistinguishable on the network and a single compromised device exposes the keypair for all. Per-device keys allow precise revocation: revoke one device without affecting others. It also enables auditability — each sync message is signed by the originating device.

### Why AES-256-GCM for vault encryption instead of ChaCha20?

Both are excellent. AES-256-GCM has hardware acceleration (AES-NI) on all modern x86/ARM processors, making it faster for large vaults on desktop/mobile. ChaCha20-Poly1305 is used for the Nostr transport layer (NIP-44) where hardware acceleration may be absent (e.g., older ARM in browser WASM context).

### Why Argon2id and not bcrypt/PBKDF2?

Argon2id is the winner of the Password Hashing Competition (2015) and is specifically designed to resist GPU and ASIC brute-force attacks through memory hardness. PBKDF2 is weak against GPU attacks. bcrypt lacks memory hardness. For a secrets manager, the KDF is the last line of defence; Argon2id is the correct choice.

### Why Tauri instead of Electron for desktop?

Tauri uses the OS's native WebView (WebKit/Blink/WebView2) rather than bundling Chromium. This results in a much smaller binary (~5 MB vs ~100+ MB) and lower memory usage. The Rust backend also means we can call `zvault-core` directly without IPC marshalling overhead.

### Why WXT for browser extensions?

Browser extensions have three major target environments (Chrome MV3, Firefox MV3/MV2, Safari) with subtle differences. WXT (Web Extension Tools) provides a unified build pipeline targeting all three from a single TypeScript codebase, similar to how React Native works for mobile. It reduces duplication and keeps browser-specific shims in one place.

### Why UniFFI for Android bindings?

UniFFI (Mozilla's Universal Foreign Function Interface) automatically generates Kotlin/Swift bindings from a Rust interface definition. This avoids writing JNI boilerplate by hand — a common source of bugs and memory safety issues. Mozilla uses it in production for Firefox's Rust components.

### Last-write-wins vs CRDT for vault items

A full CRDT (e.g., Automerge) would allow true field-level merge semantics. However, it adds significant complexity and binary size (especially in WASM). For a password manager, concurrent edits to the same item are rare. LWW at item granularity is sufficient for v1 and can be upgraded later. Device list operations (admit/revoke) do use a proper OR-Set CRDT because correctness is critical there.


---

## 19. Android App Architecture

The ZVault Android app is built with Kotlin and Jetpack Compose, using UniFFI to bridge the Rust `zvault-core` library into native Android code. The architecture follows the MVVM (Model-View-ViewModel) pattern with unidirectional data flow.

### 19.1 UI Layer — Jetpack Compose

The UI is built entirely with Jetpack Compose and Material 3 (Material You). Screens are stateless composables that receive data via parameters and emit events via callbacks:

- **UnlockScreen** — password entry, vault creation, biometric unlock trigger
- **VaultListScreen** — LazyColumn of items with search filtering and FAB
- **ItemDetailScreen** — view/edit item fields with password visibility toggle
- **AddItemScreen** — form with dynamic fields based on item kind (Login, Secure Note, Card, Identity)
- **DevicesScreen** — device trust group management (admit/revoke)
- **SettingsScreen** — biometric toggle, export/import, re-key password

Navigation is handled by Compose Navigation (`NavHost`) with a declarative route graph. The `VaultViewModel` drives navigation state: when the vault is locked, the nav graph routes to `UnlockScreen`; when unlocked, to `VaultListScreen`.

### 19.2 ViewModel Layer

`VaultViewModel` (an `AndroidViewModel`) is the single source of truth for UI state. It exposes:

- `uiState: StateFlow<VaultUiState>` — sealed interface: `Locked`, `Unlocking`, `Unlocked`, `Error`
- `items: StateFlow<List<VaultItem>>` — current vault contents
- `devices: StateFlow<List<DeviceInfo>>` — trust group members
- `selectedItem: StateFlow<VaultItem?>` — item selected for detail view

All mutation methods (`addItem`, `updateItem`, `deleteItem`, `lockVault`, etc.) launch coroutines on `Dispatchers.IO` and update the StateFlows on completion. The Compose layer observes these flows via `collectAsState()` and recomposes automatically.

### 19.3 UniFFI Bridge — `VaultRepository`

`VaultRepository` is the boundary between Kotlin and Rust. It:

1. Calls UniFFI-generated Kotlin functions (auto-generated from `zvault-core`'s UDL definition)
2. Runs all calls on `Dispatchers.IO` (Argon2id derivation is CPU-intensive)
3. Maps Rust types to Kotlin data classes (`VaultItem`, `DeviceInfo`)
4. Holds the opaque vault handle (a `Long` pointer to the Rust-side `VaultFile` + `VaultKey`)

The native library (`libzvault_core.so`) is loaded once at app startup in `ZVaultApplication.onCreate()` via `System.loadLibrary("zvault_core")`.

### 19.4 Android Keystore — Device Secret Key

Each device generates a secp256k1 keypair on first launch (via `DeviceIdentity::generate()` in Rust). The secret key bytes are stored in the Android Keystore:

- Key alias: `zvault_device_key_{device_uuid}`
- Key properties: `PURPOSE_SIGN`, hardware-backed when available (StrongBox)
- The Keystore is backed by Trusted Execution Environment (TEE) or StrongBox on supported hardware
- Secret key material never leaves the secure hardware boundary

### 19.5 BiometricPrompt — Vault Unlock

Biometric unlock wraps the `VaultKey` (derived from the master password via Argon2id) with a biometric-bound Keystore key:

1. **Enrollment:** After a successful password unlock, the 32-byte `VaultKey` is encrypted with an AES-256-GCM key stored in Keystore with `setUserAuthenticationRequired(true)` and `setInvalidatedByBiometricEnrollment(true)`.
2. **Unlock:** `BiometricPrompt` authenticates the user → Keystore releases the AES key → the wrapped `VaultKey` is decrypted → vault opens without re-running Argon2id.
3. **Invalidation:** If biometric enrollment changes (new fingerprint added), the Keystore key is invalidated and the user must re-enter their password.

This design ensures biometric unlock never weakens vault encryption — it is a convenience gate to an OS-held key blob, not an alternative to the master password.

### 19.6 WorkManager — Background Sync

Nostr sync is scheduled via `WorkManager` with the following constraints:

- **Periodic sync:** every 15 minutes (minimum interval) when network is available
- **Constraints:** `NetworkType.CONNECTED` (any network), battery not low
- **One-time sync:** triggered immediately on vault mutation or manual "sync now"
- **Retry policy:** exponential backoff on relay connection failure

The sync worker calls `zvault-core`'s sync engine via UniFFI, which builds NIP-44/NIP-59 gift-wrapped messages and publishes to configured relays.

### 19.7 AutofillService — Credential Filling

ZVault implements `android.service.autofill.AutofillService` to provide system-wide credential autofill:

1. System triggers `onFillRequest` when an app presents a login form
2. ZVault matches the requesting app's package name and web domain against stored item URIs
3. Matching credentials are presented in the autofill picker
4. On selection, the vault must be unlocked (biometric prompt if locked)
5. Username and password are filled into the requesting app's fields

Security constraints:
- Autofill responses never include TOTP secrets or notes
- URI matching is strict (exact domain, no subdomain wildcards by default)
- The vault key is required for every fill operation (no credential caching)

### 19.8 Data Flow Summary

```
┌─────────────────────────────────────────────────────────┐
│                    Compose UI Layer                       │
│  UnlockScreen │ VaultListScreen │ ItemDetail │ Settings  │
└────────────────────────┬────────────────────────────────┘
                         │ collectAsState()
                         ▼
┌─────────────────────────────────────────────────────────┐
│                   VaultViewModel                          │
│  StateFlow<VaultUiState> │ StateFlow<List<VaultItem>>    │
└────────────────────────┬────────────────────────────────┘
                         │ suspend fun (Dispatchers.IO)
                         ▼
┌─────────────────────────────────────────────────────────┐
│                   VaultRepository                         │
│  UniFFI calls │ Kotlin ↔ Rust type mapping               │
└────────────────────────┬────────────────────────────────┘
                         │ JNI / UniFFI
                         ▼
┌─────────────────────────────────────────────────────────┐
│                   zvault-core (Rust)                      │
│  Argon2id │ AES-256-GCM │ NIP-44 │ CRDT │ Vault CRUD   │
└─────────────────────────────────────────────────────────┘
```
