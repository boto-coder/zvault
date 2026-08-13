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
| Rogue/removed device tries to inject updates | All updates are signed by sender Nostr key and verified against the authorised device list |
| Removed device reads future updates | Future messages are not encrypted for removed device's public key |
| Relay replays old messages | Each message carries a monotonic `clock` (Lamport-style) and a `vault_version`; stale/replayed messages are ignored |
| Man-in-the-middle on join flow | Out-of-band verification: device pubkeys are verified via QR code or manual fingerprint check before admission |

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

### 4.3 DeviceEntry

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

### 4.4 On-disk vault file

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

## 6. Device Lifecycle

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

## 7. Conflict Resolution

Because devices can edit the vault offline simultaneously, conflicts must be resolved:

- Each vault item carries a `updated_at` timestamp and a `version` counter
- The vault as a whole uses a **last-write-wins (LWW)** strategy at item granularity (sufficient for secrets manager use cases where concurrent edits to the same item are rare)
- For device list operations (admit/revoke), a **state-based CRDT** (OR-Set) is used to ensure convergence: additions are additive; revocations are permanent tombstones that win over additions
- Full vault syncs always win over deltas if `vault_version` of the full sync is higher
- Future: consider Automerge/CRDT for field-level merge on vault items

---

## 8. Platform Architecture

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

## 9. Nostr Relay Strategy

- ZVault publishes to user-configured relays (defaults provided)
- All vault sync events use `kind: 10050` (replaceable) or custom ephemeral kinds to avoid relay storage pollution
- Events are tagged with the vault's ID (hashed/blinded) so devices can filter efficiently
- Gift-wrap (NIP-59) hides the true sender/recipient from relay operators
- Devices poll relays on startup and maintain a persistent WebSocket connection when online
- Relay list itself is stored in the vault (synced across devices)

---

## 10. Project Structure (Monorepo)

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

## 11. Technology Stack

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

## 12. Security Checklist

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

## 13. Open Questions / Future Work

1. **iOS client** — architecture is the same as Android (UniFFI + Swift), deferred to v2
2. **Field-level CRDT** — Automerge integration for true merge semantics on concurrent edits
3. **Emergency re-key UX** — needs careful UX design; currently a power-user feature
4. **Relay discovery** — how do new devices find relays if join happens before relay list is synced? (Proposal: embed relay hints in invite link)
5. **Passphrase-less unlock** — biometric unlock (Face ID, fingerprint) as an alternative to master password on mobile
6. **Organisation/shared vaults** — multi-user shared vault with role-based access; requires group encryption key scheme
7. **Import/export** — Bitwarden JSON, 1Password, LastPass, CSV import; encrypted export
8. **Audit log** — local tamper-evident log of vault access and sync events

---

## 14. Lessons Learned / Design Decisions Log

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
