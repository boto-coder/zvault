# ZVault — Component Boundaries and Architecture

## zvault-core (the only production crate so far)

Single Rust library crate. All business logic lives here. Platform-specific
code (keychain, biometrics, WebSocket) is hidden behind traits.

### Module structure and boundaries

```
zvault-core/src/
├── lib.rs        — re-exports: pub mod crypto, vault, device, nostr, sync, audit, error
├── error.rs      — Error enum + Result<T> alias; used by ALL modules
├── crypto/       — Argon2id KDF + AES-256-GCM AEAD; no knowledge of Vault or Device
├── vault/        — Vault data model + VaultFile I/O; depends on crypto
├── device/       — Device keypair + OR-Set CRDT + DeviceManager; depends on vault types
├── nostr/        — NIP-01/44/59 event construction (STUB, M4)
├── sync/         — Sync engine, conflict resolution (STUB, M4)
└── audit/        — HMAC audit log hash chain (STUB, M8)
```

### Dependency flow (no cycles)
```
error ← crypto ← vault ← device ← nostr ← sync
                  vault ← audit
```

### crypto module boundary
- Input:  password (str), plaintext (bytes), KdfParams
- Output: VaultKey (zeroized 32-byte key), encrypted blob (Vec<u8>), KdfParams
- Knows nothing about: Vault struct, Device, Nostr
- Key types: `VaultKey`, `KdfParams`

### vault module boundary
- Input:  VaultKey (from crypto), file path
- Output: Vault (in-memory model), VaultFile (disk handle)
- Knows about: crypto (uses encrypt/decrypt), DeviceEntry (defined here, used by device)
- Key types: `Vault`, `VaultItem`, `DeviceEntry`, `VaultFile`
- On-disk: atomic write (write .zvault.tmp, rename to .zvault)

### device module boundary
- Input:  SecureStorage (trait), Vault (mutated via flush)
- Output: DeviceIdentity, DeviceKeyMaterial, DeviceEntry (via DeviceManager)
- Knows about: vault types (DeviceEntry, Vault)
- Does NOT know about: crypto module (VaultKey, KdfParams)
- Key types: `DeviceIdentity`, `DeviceKeyMaterial`, `DeviceManager`, `OrSet<T>`, `SecureStorage`

### SecureStorage trait
The only cross-cutting abstraction in device module:
```rust
pub trait SecureStorage: Send + Sync {
    fn store(&self, key: &str, value: &[u8]) -> Result<()>;
    fn load(&self, key: &str) -> Result<Vec<u8>>;
    fn delete(&self, key: &str) -> Result<()>;
}
```
Storage key format: `"zvault/device/<device_id>/secret_key"`

Production implementations (future milestones):
- Desktop M5: `keyring` crate → macOS Keychain / Windows Credential / libsecret
- Android M10: Android Keystore via UniFFI
- Browser M9: browser.storage.local (encrypted)
Test implementation: `InMemoryStorage` (HashMap<String,Vec<u8>>)

## zvault-cli (stub)
Thin binary that will wrap zvault-core via clap subcommands. Not implemented
until M11. Currently contains command stubs.

## apps/ (not yet implemented)
- `desktop/` — Tauri v2 app, Phase 2 (M5+)
- `android/` — Kotlin + Jetpack Compose, Phase 3 (M10)
- `extension/` — WXT browser extension, Phase 3 (M9)

## bindings/ (not yet implemented)
- `uniffi/` — UniFFI .udl → Kotlin/Swift bindings for Android/iOS

## Data flow for vault open/edit/save
```
user provides password
    ↓
VaultFile::open(password, path)
    → parse header → KdfParams
    → derive_key(password, &kdf_params) → VaultKey
    → decrypt(key, blob) → plaintext (Zeroizing<Vec<u8>>)
    → Vault::from_json(plaintext) → Vault
    returns (VaultFile, VaultKey, Vault)
    ↓
user edits vault items
vault.add_item / update_item / delete_item
    ↓
VaultFile::save(&key, &vault)
    → vault.to_json() → Zeroizing<Vec<u8>>
    → encrypt_with_params(key, json, &kdf_params) → blob
    → atomic_write(path, blob)
    ↓
drop(key)  // Zeroizing zeroes secret on drop
```

## Data flow for device admit
```
// First device (bootstrap):
DeviceIdentity::generate(label, &storage)
    → secp256k1 keypair via AeadOsRng
    → store secret at "zvault/device/<uuid>/secret_key"
    returns (DeviceIdentity, DeviceKeyMaterial)

DeviceManager::from_vault(&vault)  // empty vault
DeviceManager::bootstrap(&material)
DeviceManager::flush(&mut vault)   // vault.devices updated, version bumped

// Second device (admit by existing device):
DeviceManager::admit(&new_material, &admin_identity)
    → check admin is live (not revoked)
    → OR-Set add with token = device_id
    → push DeviceEntry to entries
DeviceManager::flush(&mut vault)
```
