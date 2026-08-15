# Design Document: Device Key Export & Display

## Overview

Add the ability to view and export device Nostr keys across all platforms. Public keys are always visible in the device panel. Secret keys are gated behind re-authentication and shown ephemerally.

## Architecture

```
┌─────────────────────────────────────────┐
│        NIP-19 Module (NEW)              │
│  crates/zvault-core/src/nip19.rs        │
│                                         │
│  encode_npub(pubkey_bytes) -> String     │
│  encode_nsec(seckey_bytes) -> Zeroizing  │
│  decode_npub(npub_str) -> [u8; 32]      │
│  decode_nsec(nsec_str) -> Zeroizing<..>  │
└─────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────┐
│  Consumers                              │
│                                         │
│  CLI: device show / device export-key   │
│  Tauri: get_device_pubkey,              │
│         export_device_secret_key        │
│  WASM: encode_npub_from_hex             │
│  Extension: background handlers         │
└─────────────────────────────────────────┘
```

## Components and Interfaces

### Component 1: NIP-19 Codec (`crates/zvault-core/src/nip19.rs`)

**Purpose:** Encode and decode Nostr keys in bech32 (NIP-19) format.

```rust
use zeroize::Zeroizing;

/// Encode a 32-byte x-only public key as npub bech32 string.
pub fn encode_npub(pubkey: &[u8; 32]) -> String;

/// Encode a 32-byte secret key as nsec bech32 string.
pub fn encode_nsec(seckey: &[u8; 32]) -> Zeroizing<String>;

/// Decode npub bech32 string into 32 raw bytes.
pub fn decode_npub(npub: &str) -> Result<[u8; 32]>;

/// Decode nsec bech32 string into 32 raw bytes wrapped in Zeroizing.
pub fn decode_nsec(nsec: &str) -> Result<Zeroizing<[u8; 32]>>;
```

**Implementation:**
- Uses `bech32` crate (add `bech32 = "0.11"` to workspace).
- NIP-19 uses bech32 variant (NOT bech32m).
- `encode_nsec` returns `Zeroizing<String>` so the nsec string is zeroed when dropped.
- 32-byte key → 5-bit groups → 52 data chars + 6 checksum = 63 total with prefix.

### Component 2: Tauri Commands

```rust
#[derive(Serialize)]
struct DevicePubkeyInfo {
    device_id: String,
    label: String,
    pubkey_hex: String,
    npub: String,
}

#[derive(Serialize)]
struct DeviceSecretKeyInfo {
    nsec: String,
    hex: String,
}

/// Get the current device's public key info (no re-auth needed).
#[tauri::command]
fn get_device_pubkey(state: State<'_, AppState>) -> Result<DevicePubkeyInfo, String>;

/// Export the device secret key. Requires password re-verification.
#[tauri::command]
fn export_device_secret_key(password: String, state: State<'_, AppState>) -> Result<DeviceSecretKeyInfo, String>;
```

**`export_device_secret_key` flow:**
1. Verify `password` matches vault (call `derive_key` + compare or attempt decrypt)
2. Load secret key from SecureStorage → `Zeroizing<Vec<u8>>`
3. Encode as nsec → `Zeroizing<String>`
4. Encode as hex
5. Return both (serialized to frontend)
6. Drop all Zeroizing values

### Component 3: WASM Exports

```rust
/// Convert 64-char hex pubkey to npub bech32.
#[wasm_bindgen]
pub fn encode_npub_from_hex(pubkey_hex: &str) -> Result<String, JsValue>;
```

Secret key export in the extension goes through the background script, which loads the encrypted secret from `browser.storage.local`, decrypts it, encodes via WASM, and returns to the popup. No secret key WASM export function needed — the background script orchestrates.

### Component 4: CLI Subcommands

Add to `DeviceAction` enum:

```rust
/// Show device identity (pubkey in hex + npub).
Show {
    #[arg(long, short, env = "ZVAULT_PATH")]
    vault: PathBuf,
},

/// Export device secret key (nsec + hex). Requires password.
ExportKey {
    #[arg(long, short, env = "ZVAULT_PATH")]
    vault: PathBuf,
},
```

### Component 5: React UI

**DevicePubkeyCard** (in "My Device" section):
- Device label + ID
- Hex pubkey (truncated to first/last 8 chars, "Copy full" button)
- npub (shown full, "Copy" button)
- Compact single-row layout

**ExportSecretKeyDialog** (modal):
- Warning text + password input
- On submit: calls backend, shows nsec + hex with copy buttons
- 30-second countdown auto-dismiss
- "Done" button for immediate dismiss

## Security Considerations

1. **nsec zeroing:** The `Zeroizing<String>` wrapper ensures the nsec string is zeroed in Rust when dropped. In the frontend, React state is cleared on dialog dismiss.
2. **Re-auth for export:** Prevents walk-up attacks on unlocked machines.
3. **No logging:** Secret keys are never included in tracing output.
4. **Ephemeral display:** 30-second auto-hide reduces exposure window.
5. **Public key is free:** No special protection needed for npub/hex pubkey.

## Dependencies

### New Crate

```toml
# Add to [workspace.dependencies] in root Cargo.toml
bech32 = "0.11"
```

Add to `zvault-core`'s `[dependencies]`.

## Testing Strategy

- **Unit:** NIP-19 encode/decode round-trip for random 32-byte arrays
- **Test vectors:** Known npub/nsec pairs from the NIP-19 spec or reference implementations
- **Property test:** `decode(encode(x)) == x` for all random keys
- **CLI integration:** `device show` prints correct npub for a known keypair
- **CLI integration:** `device export-key` requires password, prints correct nsec
- **Security:** Verify nsec is NOT present in log output after export
