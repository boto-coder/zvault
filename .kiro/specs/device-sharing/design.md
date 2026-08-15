# Design Document: Device Sharing (Invite & Join-Request Flows)

## Overview

This design replaces the manual "paste a 64-char hex public key" device pairing UX with two user-friendly flows:

1. **Invite flow (A → B):** An existing device generates a shareable invite code (QR/text/link). The new device imports it, auto-admits the inviter, and generates a response code. The inviter imports the response and admits the new device. Two-step handshake, mutual admission.

2. **Join-request flow (B → A):** A new device generates a join-request code. The user gives it to an existing device. The existing device admits the new device and generates a response. The new device imports the response and admits the existing device.

Both flows produce the exact same `DeviceEntry` records in the vault as the existing manual method. The core `DeviceManager`, OR-Set CRDT, and sync engine are unchanged — this is purely a UX/encoding layer on top.

## Architecture

```
┌──────────────────────────────────────────────────────┐
│                    Pairing Layer (NEW)                │
│                                                      │
│  ┌─────────────────┐      ┌──────────────────────┐  │
│  │  PairingCodec   │      │  PairingOrchestrator │  │
│  │  (encode/decode)│      │  (state machine)     │  │
│  └─────────────────┘      └──────────────────────┘  │
│                                                      │
└──────────────────────────────────────────────────────┘
        │                           │
        ▼                           ▼
┌──────────────────────────────────────────────────────┐
│               Existing Device Layer                  │
│                                                      │
│  DeviceIdentity::generate()  DeviceManager::admit()  │
│  SecureStorage               VaultFile::save()       │
│                                                      │
└──────────────────────────────────────────────────────┘
```

### Component Breakdown

1. **PairingCodec** (new, in `zvault-core`): Encodes/decodes pairing payloads to/from `zvault:` base64url strings. Pure functions, no state.

2. **PairingOrchestrator** (new, in each frontend): Guides the user through the multi-step pairing flow. Tracks which step we're on (generated invite → waiting for response → complete).

3. **QR Renderer** (frontend): Renders pairing codes as QR codes. Desktop/extension use a JS library (e.g. `qrcode`). Android uses native camera + ZXing.

4. **Existing device layer**: `DeviceIdentity`, `DeviceManager`, `SecureStorage`, `VaultFile` — all unchanged.

## Pairing Code Format

### Encoding

All pairing codes use the format:

```
zvault:<base64url-encoded-JSON>
```

Example: `zvault:eyJ2IjoxLCJ0IjoiaW52aXRlIiwicCI6ImFiY2QuLi4iLCJsIjoiTXkgRGVza3RvcCIsInZpZCI6IjEyMzQifQ`

### Payload Schema

```typescript
// Version 1 payload
interface PairingPayload {
  v: 1;                          // Version (for forward compat)
  t: "invite" | "join_request" | "invite_response" | "join_response";
  p: string;                     // Public key (64 hex chars)
  l: string;                     // Device label
  vid?: string;                  // Vault ID (present in invite and invite_response)
  ts: number;                    // Unix timestamp (seconds)
}
```

**Field sizes:**
- `v`: 1 byte
- `t`: 4–15 bytes
- `p`: 64 bytes (hex pubkey)
- `l`: 1–64 bytes
- `vid`: 36 bytes (UUID) or absent
- `ts`: 10 bytes (unix seconds)

Total JSON: ~130–200 bytes → base64url: ~180–270 chars → with `zvault:` prefix: under 300 chars. Well within QR code capacity (standard alphanumeric QR fits ~4000 chars).

### Code Types

| Type | Who generates | Contains | Purpose |
|------|--------------|----------|---------|
| `invite` | Existing device (A) | A's pubkey, label, vault_id | A invites a new device |
| `join_response` | New device (B) | B's pubkey, label | B responds to A's invite |
| `join_request` | New device (B) | B's pubkey, label | B asks to join A's vault |
| `invite_response` | Existing device (A) | A's pubkey, label, vault_id | A responds to B's join request |

## Sequence Diagrams

### Flow 1: A invites B

```
Device A (existing)                    Device B (new)
─────────────────                      ──────────────
1. Click "Invite Device"
2. Generate invite code
   {t:"invite", p:A_pub, l:A_label, vid:...}
3. Show QR / copy text ──────────────► 4. Scan/paste invite code
                                        5. Decode → show A's info
                                        6. User confirms
                                        7. Generate own identity (if needed)
                                        8. Admit A into B's device list
                                        9. Generate join_response code
                                           {t:"join_response", p:B_pub, l:B_label}
10. Scan/paste response ◄────────────── 10. Show QR / copy text
11. Decode → show B's info
12. User confirms
13. Admit B into A's device list
14. ✅ Pairing complete                 14. ✅ Pairing complete
15. Trigger sync to B                  15. Ready to receive sync
```

### Flow 2: B requests to join A

```
Device B (new)                         Device A (existing)
──────────────                         ─────────────────
1. Click "Request to Join"
2. Generate own identity (if needed)
3. Generate join_request code
   {t:"join_request", p:B_pub, l:B_label}
4. Show QR / copy text ──────────────► 5. Scan/paste join_request
                                        6. Decode → show B's info
                                        7. User confirms
                                        8. Admit B into A's device list
                                        9. Generate invite_response code
                                           {t:"invite_response", p:A_pub, l:A_label, vid:...}
10. Scan/paste response ◄────────────── 10. Show QR / copy text
11. Decode → show A's info
12. User confirms
13. Admit A into B's device list
14. ✅ Pairing complete                 14. ✅ Pairing complete
                                        15. Trigger sync to B
```

## Components and Interfaces

### Component 1: PairingCodec (zvault-core)

**Purpose:** Encode and decode pairing payloads. Pure, stateless, no crypto beyond validation.

**Location:** `crates/zvault-core/src/pairing.rs` (new module)

**Interface:**

```rust
/// The pairing payload carried inside a `zvault:` code.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingPayload {
    pub v: u8,
    pub t: PairingType,
    pub p: String,       // 64-char hex pubkey
    pub l: String,       // device label
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vid: Option<String>,  // vault ID (UUID string)
    pub ts: i64,         // unix timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PairingType {
    Invite,
    JoinRequest,
    InviteResponse,
    JoinResponse,
}

/// Encode a PairingPayload into a `zvault:...` string.
pub fn encode_pairing_code(payload: &PairingPayload) -> Result<String>;

/// Decode a `zvault:...` string into a PairingPayload.
/// Validates: prefix, base64url, JSON structure, pubkey format, label length, version.
pub fn decode_pairing_code(code: &str) -> Result<PairingPayload>;

/// Generate an invite payload for the current device.
pub fn create_invite(pubkey_hex: &str, label: &str, vault_id: &str) -> PairingPayload;

/// Generate a join_request payload for the current device.
pub fn create_join_request(pubkey_hex: &str, label: &str) -> PairingPayload;

/// Generate a join_response payload (response to an invite).
pub fn create_join_response(pubkey_hex: &str, label: &str) -> PairingPayload;

/// Generate an invite_response payload (response to a join_request).
pub fn create_invite_response(pubkey_hex: &str, label: &str, vault_id: &str) -> PairingPayload;
```

**Validation in `decode_pairing_code`:**
- Must start with `zvault:`
- Remainder must be valid base64url
- Decoded bytes must be valid UTF-8 JSON
- `v` must be `1` (reject unknown versions with descriptive error)
- `t` must be a known PairingType
- `p` must be exactly 64 hex characters
- `l` must be 1–64 chars after trim
- `vid` if present must be valid UUID format
- `ts` must be a reasonable Unix timestamp (> 0)

### Component 2: Tauri Commands (Desktop)

**New commands added to `apps/desktop/src-tauri/src/main.rs`:**

```rust
/// Generate an invite code for the current device.
#[tauri::command]
fn create_invite_code(state: State<'_, AppState>) -> Result<PairingCodeResult, String>;

/// Generate a join-request code for the current device.
#[tauri::command]
fn create_join_request_code(state: State<'_, AppState>) -> Result<PairingCodeResult, String>;

/// Import any pairing code (auto-detects type) and return what action is needed.
#[tauri::command]
fn import_pairing_code(code: String, state: State<'_, AppState>) -> Result<PairingImportResult, String>;

/// Confirm admission of the device described in the import result.
/// Returns a response code if one is needed (e.g., join_response after importing an invite).
#[tauri::command]
fn confirm_pairing(payload: PairingPayload, state: State<'_, AppState>) -> Result<PairingConfirmResult, String>;
```

**DTOs:**

```rust
#[derive(Serialize)]
struct PairingCodeResult {
    code: String,           // The full zvault:... string
    payload: PairingPayload, // For display purposes
}

#[derive(Serialize)]
struct PairingImportResult {
    payload: PairingPayload,
    action_needed: String,  // "confirm_and_respond" | "confirm_final"
    description: String,    // Human-readable: "Device 'Alice's MacBook' wants to join..."
}

#[derive(Serialize)]
struct PairingConfirmResult {
    admitted_device: DeviceSummary,
    response_code: Option<String>,  // Present if the other device needs our response
}
```

### Component 3: WASM Bindings (Extension)

**New exports in `crates/zvault-wasm/src/lib.rs`:**

```rust
#[wasm_bindgen]
pub fn create_invite_code(pubkey_hex: &str, label: &str, vault_id: &str) -> Result<String, JsValue>;

#[wasm_bindgen]
pub fn create_join_request_code(pubkey_hex: &str, label: &str) -> Result<String, JsValue>;

#[wasm_bindgen]
pub fn decode_pairing_code(code: &str) -> Result<JsValue, JsValue>;

#[wasm_bindgen]
pub fn admit_device_from_pairing(
    vault_json: &str,
    admin_device_id: &str,
    admin_pubkey_hex: &str,
    remote_pubkey_hex: &str,
    remote_label: &str,
) -> Result<String, JsValue>;

#[wasm_bindgen]
pub fn create_response_code(
    response_type: &str,  // "join_response" or "invite_response"
    pubkey_hex: &str,
    label: &str,
    vault_id: Option<String>,
) -> Result<String, JsValue>;
```

### Component 4: Extension Background Handlers

**New message types in `apps/extension/src/entrypoints/background.ts`:**

```typescript
type PairingMessage =
  | { type: "CREATE_INVITE_CODE" }
  | { type: "CREATE_JOIN_REQUEST_CODE" }
  | { type: "IMPORT_PAIRING_CODE"; payload: { code: string } }
  | { type: "CONFIRM_PAIRING"; payload: { remotePayload: PairingPayload } };
```

### Component 5: React UI — Pairing Flow Components

**New components:**

- `InviteDeviceDialog` — Generates and displays invite code (QR + text). Then shows input for response code.
- `JoinRequestDialog` — Generates and displays join-request code. Then shows input for response code.
- `ImportCodeInput` — Universal paste input that accepts any `zvault:` code and auto-detects type.
- `ConfirmPairingDialog` — Shows the remote device's info and asks for confirmation before admitting.
- `QRCodeDisplay` — Renders a string as a QR code (uses `qrcode.react` or similar).
- `PairingSuccess` — Success confirmation screen after both sides complete.

**Updated Device_Panel layout:**

```
┌────────────────────────────────────────────────┐
│  My Device                                     │
│  Label: Alice's Desktop                        │
│  Pubkey: abcd1234...ef56 [Copy]               │
├────────────────────────────────────────────────┤
│  [Invite Device]  [Request to Join]  [Paste Code]│
├────────────────────────────────────────────────┤
│  Paired Devices                                │
│  ┌──────────────────────────────────────────┐  │
│  │ Bob's Phone    active   [Revoke]        │  │
│  │ Chrome Ext     active   [Revoke]        │  │
│  │ Old Laptop     revoked                  │  │
│  └──────────────────────────────────────────┘  │
├────────────────────────────────────────────────┤
│  Advanced: [Admit by pubkey]                   │
└────────────────────────────────────────────────┘
```

## Data Models

### PairingPayload

```rust
pub struct PairingPayload {
    pub v: u8,                    // Always 1
    pub t: PairingType,           // invite | join_request | invite_response | join_response
    pub p: String,                // 64-char hex pubkey
    pub l: String,                // 1–64 char label
    pub vid: Option<String>,      // UUID vault_id (for invite and invite_response)
    pub ts: i64,                  // Unix seconds
}
```

### Vault ID

The vault ID (`vid`) is a UUID stored in the `Vault` struct. If not already present, it's generated on first vault creation. This allows codes to reference a specific vault (preventing a user from accidentally importing a code meant for a different vault).

**Addition to `Vault` struct:**

```rust
pub struct Vault {
    // ... existing fields ...
    pub vault_id: Uuid,  // NEW: stable vault identifier, generated at create time
}
```

## Algorithmic Pseudocode

### Encode Pairing Code

```rust
fn encode_pairing_code(payload: &PairingPayload) -> Result<String> {
    // 1. Serialize to compact JSON (no extra whitespace)
    let json = serde_json::to_string(payload)?;

    // 2. Base64url encode (no padding)
    let encoded = base64url_encode(json.as_bytes());

    // 3. Prefix
    Ok(format!("zvault:{encoded}"))
}
```

### Decode Pairing Code

```rust
fn decode_pairing_code(code: &str) -> Result<PairingPayload> {
    // 1. Check prefix
    let encoded = code.strip_prefix("zvault:")
        .ok_or(Error::InvalidPairingCode("missing zvault: prefix"))?;

    // 2. Base64url decode
    let bytes = base64url_decode(encoded)
        .map_err(|_| Error::InvalidPairingCode("invalid base64url"))?;

    // 3. Parse JSON
    let payload: PairingPayload = serde_json::from_slice(&bytes)
        .map_err(|_| Error::InvalidPairingCode("invalid JSON payload"))?;

    // 4. Validate fields
    if payload.v != 1 {
        return Err(Error::InvalidPairingCode("unsupported version"));
    }
    if payload.p.len() != 64 || !payload.p.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::InvalidPairingCode("invalid public key format"));
    }
    let label = payload.l.trim();
    if label.is_empty() || label.len() > 64 {
        return Err(Error::InvalidPairingCode("label must be 1-64 characters"));
    }
    if let Some(ref vid) = payload.vid {
        Uuid::parse_str(vid)
            .map_err(|_| Error::InvalidPairingCode("invalid vault_id UUID"))?;
    }

    Ok(payload)
}
```

### Desktop: Create Invite Code

```rust
fn create_invite_code(state: State<AppState>) -> Result<PairingCodeResult, String> {
    let session = state.session.lock()?.as_ref().ok_or("Vault is locked")?;
    let identity = state.device_identity.lock()?.as_ref().ok_or("No device identity")?;

    let payload = pairing::create_invite(
        &identity.pubkey_hex,
        &get_device_label(&session.vault, identity.device_id),
        &session.vault.vault_id.to_string(),
    );

    let code = pairing::encode_pairing_code(&payload)?;

    Ok(PairingCodeResult { code, payload })
}
```

### Desktop: Import Pairing Code

```rust
fn import_pairing_code(code: String, state: State<AppState>) -> Result<PairingImportResult, String> {
    let _ = state.session.lock()?.as_ref().ok_or("Vault is locked")?;

    let payload = pairing::decode_pairing_code(&code)?;

    let (action_needed, description) = match payload.t {
        PairingType::Invite => (
            "confirm_and_respond",
            format!("'{}' is inviting you to join their vault.", payload.l),
        ),
        PairingType::JoinRequest => (
            "confirm_and_respond",
            format!("'{}' is requesting to join your vault.", payload.l),
        ),
        PairingType::JoinResponse | PairingType::InviteResponse => (
            "confirm_final",
            format!("'{}' has accepted your pairing request.", payload.l),
        ),
    };

    Ok(PairingImportResult { payload, action_needed: action_needed.into(), description })
}
```

### Desktop: Confirm Pairing

```rust
fn confirm_pairing(payload: PairingPayload, state: State<AppState>) -> Result<PairingConfirmResult, String> {
    let session = state.session.lock()?.as_mut().ok_or("Vault is locked")?;
    let identity = state.device_identity.lock()?.as_ref().ok_or("No device identity")?;

    // Admit the remote device
    let material = DeviceKeyMaterial {
        device_id: Uuid::new_v4(),
        label: payload.l.clone(),
        pubkey_hex: payload.p.clone(),
    };

    let mut dm = DeviceManager::from_vault(&session.vault);
    let entry = dm.admit(&material, identity)?;
    dm.flush(&mut session.vault);
    session.vault_file.save(&session.key, &session.vault)?;

    // Generate response code if needed
    let response_code = match payload.t {
        PairingType::Invite => {
            // We're B responding to A's invite → generate join_response
            let resp = pairing::create_join_response(
                &identity.pubkey_hex,
                &get_device_label(&session.vault, identity.device_id),
            );
            Some(pairing::encode_pairing_code(&resp)?)
        }
        PairingType::JoinRequest => {
            // We're A responding to B's join request → generate invite_response
            let resp = pairing::create_invite_response(
                &identity.pubkey_hex,
                &get_device_label(&session.vault, identity.device_id),
                &session.vault.vault_id.to_string(),
            );
            Some(pairing::encode_pairing_code(&resp)?)
        }
        PairingType::JoinResponse | PairingType::InviteResponse => {
            // Final step — no further response needed
            None
        }
    };

    Ok(PairingConfirmResult {
        admitted_device: DeviceSummary::from(&entry),
        response_code,
    })
}
```

## Error Handling

| Error Condition | Response | Recovery |
|----------------|----------|----------|
| Vault locked | "Vault is locked" | Prompt user to unlock |
| No device identity | "No device identity — generate one first" | Show identity generation UI |
| Invalid code prefix | "Invalid pairing code: must start with zvault:" | Show input error |
| Invalid base64url | "Invalid pairing code: could not decode" | Show input error |
| Invalid JSON | "Invalid pairing code: malformed payload" | Show input error |
| Unsupported version | "This pairing code uses a newer format. Update ZVault." | Show update prompt |
| Invalid pubkey in code | "Invalid pairing code: public key must be 64 hex chars" | Show input error |
| Admin revoked | "Your device has been revoked and cannot admit new devices" | Show status |
| Duplicate pubkey | "This device is already in your trust group" | Show existing device |

## Security Considerations

1. **No secrets in codes:** Pairing codes contain ONLY public keys and metadata. Intercepting a code reveals nothing about vault contents or private keys.

2. **Mutual admission required:** Both devices must explicitly confirm before sync works. A single intercepted code cannot grant vault access.

3. **No replay risk:** Importing the same code twice results in "device already admitted" — not a second entry. The pubkey is used for deduplication.

4. **No expiry in v1:** Codes don't expire. This is an accepted trade-off for simplicity. A future version could add optional expiry.

5. **Vault ID prevents cross-vault confusion:** The `vid` field lets the receiving device verify the invite matches the expected vault (informational warning only — not enforced in v1).

## Testing Strategy

### Unit Tests (zvault-core pairing module)
- Encode → decode round-trip for all 4 code types
- Decode rejects: missing prefix, invalid base64, invalid JSON, bad version, bad pubkey, bad label, bad UUID
- `create_invite` / `create_join_request` produce valid payloads
- Payload size stays under 300 chars for typical inputs

### Integration Tests
- Full invite flow: A creates invite → B imports → B confirms (A admitted, response generated) → A imports response → A confirms (B admitted)
- Full join-request flow: B creates request → A imports → A confirms (B admitted, response generated) → B imports response → B confirms (A admitted)
- After both flows: both vaults have identical device lists (2 devices each)

### Property Tests
- For all random valid pubkeys and labels: encode then decode produces the original payload
- For all random strings: decode either succeeds with a valid payload or returns a descriptive error (never panics)

## Dependencies

### New Dependencies

- `base64` (already in workspace) — used for base64url encoding/decoding
- `qrcode.react` or `qrcode` (npm, frontend only) — QR code rendering in React

### No New Rust Crates

The pairing module uses only `serde`, `serde_json`, `base64`, `uuid`, and `chrono` — all already in the workspace.

## Performance Considerations

- Pairing code generation is instant (no crypto, just JSON + base64).
- QR rendering is client-side and fast for strings under 500 chars.
- Device admission (the actual crypto/CRDT work) happens only on confirm — same performance as today.
