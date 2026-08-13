# ZVault — Key Data Structures and On-Disk Formats

## On-disk vault file format (.zvault)

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

Total fixed overhead: 80 bytes (64-byte header + 16-byte tag).

The FULL header (magic + salt + m/t/p_cost + IV) is used as AES-GCM AAD.
Any tampering with the header is detected before decryption.

## Default Argon2id parameters
- m_cost: 65536 KiB (64 MiB) — OWASP interactive login baseline
- t_cost: 3 iterations
- p_cost: 4 parallel threads
Test parameters: m_cost=8, t_cost=1, p_cost=1 (for speed)

## VaultKey
```rust
pub struct VaultKey(Zeroizing<[u8; 32]>);
```
- Does NOT derive Clone or Copy (prevents accidental duplication)
- Zeroizing guarantees memory is zeroed on drop
- Produced by derive_key(password, &KdfParams)

## KdfParams
```rust
pub struct KdfParams {
    pub salt: [u8; 32],   // random per vault-create or rekey
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}
```
Stored in the vault file header. Also held in VaultFile for save correctness.

## VaultFile (in-memory handle)
```rust
pub struct VaultFile {
    path: PathBuf,         // canonical path to .zvault file
    kdf_params: KdfParams, // stored KdfParams — used by save()
}
```
Critical: save() uses encrypt_with_params(&kdf_params) NOT encrypt().
Using encrypt() would generate a new salt, making the held VaultKey invalid.

## Vault (in-memory, decrypted)
```rust
pub struct Vault {
    id: Uuid,           // stable vault identifier, never changes
    version: u64,       // incremented on EVERY mutation
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    items: Vec<VaultItem>,
    devices: Vec<DeviceEntry>,
}
```
Serialised to JSON → encrypted → written to .zvault file.
version is the ONLY valid conflict signal (not timestamps) for M4 sync.

## VaultItem
```rust
pub struct VaultItem {
    id: Uuid,
    kind: ItemKind,  // Login | SecureNote | Card | Identity
    name: String,
    // ... kind-specific optional fields
    password: Option<String>,   // zeroed on drop
    totp_secret: Option<String>, // zeroed on drop
    note: Option<String>,        // zeroed on drop
    card_number: Option<String>, // zeroed on drop
    cvv: Option<String>,         // zeroed on drop
}
```
Manual Drop impl zeroes all sensitive String fields.
Derives Clone (accepted risk — re-evaluate at M5).

## DeviceEntry (stored in Vault::devices)
```rust
pub struct DeviceEntry {
    device_id: Uuid,
    nostr_pubkey: String,  // x-only secp256k1 pubkey, 64 hex chars
    label: String,
    added_at: DateTime<Utc>,
    added_by: Uuid,        // which device admitted this one
    revoked: bool,
    revoked_at: Option<DateTime<Utc>>,
    revoked_by: Option<Uuid>,
}
```

## DeviceIdentity (in-memory only, not serialised)
```rust
pub struct DeviceIdentity {
    device_id: Uuid,
    pubkey_hex: String,  // x-only public key, 64 hex chars
}
```
Secret key is NEVER in this struct — lives in SecureStorage only.
Storage key: "zvault/device/<device_id>/secret_key"

## OrSet<T> (OR-Set CRDT)
```rust
pub struct OrSet<T> {
    adds: Vec<(T, Uuid)>,               // live elements with unique tokens
    removes: HashSet<Uuid>,             // tokens that have been removed
}
```
- add(element) → token (Uuid::new_v4())
- remove(element) → moves tokens to removes set
- merge(other) → union removes, union adds (dedup by token), filter by removes
- For DeviceManager: token = device_id (deterministic, not random)

## Secure storage key paths
- Device secret key: `"zvault/device/<device_id>/secret_key"`
- (Future M6) Biometric wrapped vault key: TBD

## Atomic write pattern
```rust
// Appends .tmp to full filename (NOT with_extension which replaces)
let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
tmp_name.push(".tmp");
let tmp = path.with_file_name(tmp_name);
// Write to tmp, then fs::rename(tmp, path)
```
my.zvault → my.zvault.tmp (rename-atomic on same filesystem)
