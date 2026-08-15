# Design Document: UX Improvements

## Architecture Overview

This spec adds multiple UI surfaces and backend additions to both the Firefox extension and the Tauri desktop app:

**Extension additions:**
- Item detail view (new route)
- Search/filter in item list
- Suggested items (current-tab domain matching)
- Toast notification system
- Device management view with invite flow
- Real password copy (fix placeholder)
- Kind icons + visual hierarchy

**Desktop additions:**
- Full add-item form (replaces name+kind modal)
- Password generator (Tauri command + UI)
- Delete confirmation modal
- Keyboard shortcuts
- Device management view with admit/revoke/invite flow
- "Show my pubkey" + copy

```
Extension Popup Views (updated)
─────────────────────────────────
  loading → create | unlock → items → item-detail
                                    → devices
                                    → create-item (from existing spec)

Desktop App Views (updated)
─────────────────────────────────
  unlock → list → detail
                → devices
           ↑ Add Item (full modal)
           ↑ Keyboard shortcuts (global)
```

## Components

### 1. Extension Toast Component

```typescript
interface ToastProps {
  message: string;
  visible: boolean;
}

function Toast({ message, visible }: ToastProps) {
  if (!visible) return null;
  return (
    <div role="status" style={{
      position: "fixed", top: "8px", left: "50%", transform: "translateX(-50%)",
      background: "#0f3460", color: "#e0e0e0", padding: "0.5rem 1rem",
      borderRadius: "4px", fontSize: "0.85rem", zIndex: 1000,
      boxShadow: "0 2px 8px rgba(0,0,0,0.3)",
    }}>
      {message}
    </div>
  );
}
```

Usage: managed via `useState` in the App component with a `showToast(msg)` helper that sets visibility and auto-clears after 2 seconds.

### 2. Extension Item Detail View

New route `"item-detail"` with state `{ itemId: string }`. Fetches full item data via a new `GET_ITEM` background message.

```typescript
function ItemDetailView({
  itemId,
  onBack,
  showToast,
}: {
  itemId: string;
  onBack: () => void;
  showToast: (msg: string) => void;
}) {
  const [item, setItem] = useState<FullVaultItem | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const [showCvv, setShowCvv] = useState(false);

  useEffect(() => {
    browser.runtime.sendMessage({ type: "GET_ITEM", payload: { id: itemId } })
      .then(res => { if (res.item) setItem(res.item); });
  }, [itemId]);

  const copyField = async (value: string, label: string) => {
    await navigator.clipboard.writeText(value);
    showToast(`${label} copied!`);
    setTimeout(() => navigator.clipboard.writeText(""), 30000);
  };

  // ... render fields based on item.kind
}
```

### 3. Extension Background Handlers (new)

```typescript
case "GET_PASSWORD": {
  if (!sessionVaultJson) return { error: "Vault is locked" };
  const { id } = message.payload as { id: string };
  const vault = JSON.parse(sessionVaultJson);
  const item = vault.items.find((i: any) => i.id === id);
  return item ? { password: item.password || null } : { error: "Item not found" };
}

case "GET_ITEM": {
  if (!sessionVaultJson) return { error: "Vault is locked" };
  const { id } = message.payload as { id: string };
  const vault = JSON.parse(sessionVaultJson);
  const item = vault.items.find((i: any) => i.id === id);
  return item ? { item } : { error: "Item not found" };
}

case "LIST_DEVICES": {
  if (!sessionVaultJson) return { error: "Vault is locked" };
  const vault = JSON.parse(sessionVaultJson);
  return { devices: vault.devices || [] };
}

case "ADMIT_DEVICE": {
  if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
  const { pubkeyHex, label } = message.payload as { pubkeyHex: string; label: string };
  // Validate pubkey format (64 hex chars)
  if (!/^[0-9a-f]{64}$/i.test(pubkeyHex)) return { error: "Invalid public key format" };
  const vault = JSON.parse(sessionVaultJson);
  const entry = {
    device_id: crypto.randomUUID(),
    nostr_pubkey: pubkeyHex.toLowerCase(),
    label,
    added_at: new Date().toISOString(),
    added_by: vault.devices?.[0]?.device_id || "unknown",
    revoked: false,
  };
  vault.devices = vault.devices || [];
  vault.devices.push(entry);
  vault.version = (vault.version || 0) + 1;
  sessionVaultJson = JSON.stringify(vault);
  // Re-encrypt and persist
  const { initWasm } = await import("../lib/wasm");
  const wasm = await initWasm();
  const encrypted = wasm.encrypt_vault(sessionPassword, sessionVaultJson);
  await browser.storage.local.set({ vault: Array.from(encrypted) });
  return { success: true, deviceId: entry.device_id };
}

case "REVOKE_DEVICE": {
  if (!sessionVaultJson || !sessionPassword) return { error: "Vault is locked" };
  const { deviceId } = message.payload as { deviceId: string };
  const vault = JSON.parse(sessionVaultJson);
  const device = vault.devices?.find((d: any) => d.device_id === deviceId);
  if (!device) return { error: "Device not found" };
  device.revoked = true;
  device.revoked_at = new Date().toISOString();
  vault.version = (vault.version || 0) + 1;
  sessionVaultJson = JSON.stringify(vault);
  const { initWasm } = await import("../lib/wasm");
  const wasm = await initWasm();
  const encrypted = wasm.encrypt_vault(sessionPassword, sessionVaultJson);
  await browser.storage.local.set({ vault: Array.from(encrypted) });
  return { success: true };
}
```

### 4. Extension Device Management View

```typescript
function DevicesView({
  onBack,
  showToast,
}: {
  onBack: () => void;
  showToast: (msg: string) => void;
}) {
  const [devices, setDevices] = useState<DeviceEntry[]>([]);
  const [showAdmit, setShowAdmit] = useState(false);
  const [myPubkey, setMyPubkey] = useState<string | null>(null);

  // ... load devices, show list, admit/revoke actions
}
```

### 5. Desktop Device Management View (`apps/desktop/src/pages/Devices.tsx`)

New page accessible from VaultList header. Uses existing `list_devices` Tauri command. New Tauri commands needed:

```rust
#[tauri::command]
fn admit_device(pubkey_hex: String, label: String, state: State<'_, AppState>) -> Result<String, String> {
    // Validate pubkey format
    if pubkey_hex.len() != 64 || !pubkey_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid public key: must be 64 hex characters".into());
    }
    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    let device_id = Uuid::new_v4();
    let entry = zvault_core::vault::DeviceEntry {
        device_id,
        nostr_pubkey: pubkey_hex.to_lowercase(),
        label,
        added_at: chrono::Utc::now(),
        added_by: session.vault.devices.first().map(|d| d.device_id).unwrap_or(device_id),
        revoked: false,
        revoked_at: None,
        revoked_by: None,
    };
    session.vault.devices.push(entry);
    session.vault.version += 1;
    session.vault.updated_at = chrono::Utc::now();
    session.vault_file.save(&session.key, &session.vault).map_err(|e| e.to_string())?;

    Ok(device_id.to_string())
}

#[tauri::command]
fn revoke_device(device_id: String, state: State<'_, AppState>) -> Result<(), String> {
    let uuid = Uuid::parse_str(&device_id).map_err(|e| e.to_string())?;
    let mut session = state.session.lock().map_err(|e| e.to_string())?;
    let session = session.as_mut().ok_or("Vault is locked")?;

    let entry = session.vault.devices.iter_mut()
        .find(|d| d.device_id == uuid)
        .ok_or("Device not found")?;

    if entry.revoked {
        return Err("Device already revoked".into());
    }
    entry.revoked = true;
    entry.revoked_at = Some(chrono::Utc::now());
    session.vault.version += 1;
    session.vault.updated_at = chrono::Utc::now();
    session.vault_file.save(&session.key, &session.vault).map_err(|e| e.to_string())?;

    Ok(())
}
```

### 6. Desktop Password Generator Command

```rust
#[tauri::command]
fn generate_password(length: Option<u32>) -> Result<String, String> {
    let len = length.unwrap_or(20) as usize;
    if len < 4 {
        return Err("Minimum password length is 4".into());
    }

    use aes_gcm::aead::OsRng as AeadOsRng;
    use aes_gcm::aead::rand_core::RngCore as _;

    const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    const DIGITS: &[u8] = b"0123456789";
    const SPECIAL: &[u8] = b"!@#$%^&*()_+-=[]{}|;:,.<>?";
    const ALL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+-=[]{}|;:,.<>?";

    let mut password = Vec::with_capacity(len);
    let mut random_bytes = vec![0u8; len + 4];
    AeadOsRng.fill_bytes(&mut random_bytes);

    // Guarantee one from each class
    password.push(UPPER[(random_bytes[0] as usize) % UPPER.len()]);
    password.push(LOWER[(random_bytes[1] as usize) % LOWER.len()]);
    password.push(DIGITS[(random_bytes[2] as usize) % DIGITS.len()]);
    password.push(SPECIAL[(random_bytes[3] as usize) % SPECIAL.len()]);

    // Fill remaining
    for i in 4..len {
        password.push(ALL[(random_bytes[i] as usize) % ALL.len()]);
    }

    // Fisher-Yates shuffle
    let mut shuffle_bytes = vec![0u8; len];
    AeadOsRng.fill_bytes(&mut shuffle_bytes);
    for i in (1..len).rev() {
        let j = (shuffle_bytes[i] as usize) % (i + 1);
        password.swap(i, j);
    }

    Ok(String::from_utf8(password).expect("all chars are ASCII"))
}
```

### 7. Desktop Full Add Item Form

Replaces the existing `AddItemModal` (name + kind only) with a full form. Same structure as the extension `ItemCreateView` from the existing spec but using Tailwind styles:

- Type selector dropdown (Login, Secure Note, Card, Identity)
- Type-specific fields conditionally rendered
- Password field with "Generate" button (calls `invoke("generate_password")`)
- TOTP secret field with validation
- URI list (add/remove) for login items
- Submit builds full `ItemInput` JSON and calls existing `add_item` Tauri command

### 8. Desktop Keyboard Shortcuts

Registered in `App.tsx` via `useEffect`:

```typescript
useEffect(() => {
  if (view.page === "unlock") return; // Only active when unlocked

  const handler = (e: KeyboardEvent) => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key === "l") { e.preventDefault(); handleLock(); }
    if (mod && e.key === "n") { e.preventDefault(); openAddModal(); }
    if (mod && e.key === "f") { e.preventDefault(); focusSearch(); }
    if (e.key === "Escape") { closeModals(); }
  };
  window.addEventListener("keydown", handler);
  return () => window.removeEventListener("keydown", handler);
}, [view]);
```

## Invite Flow UX (Both Platforms)

The invite flow for admitting a new device to the trust group:

```
┌─────────────────────────────────────────────────────────┐
│  Device A (existing)           Device B (new)           │
│                                                          │
│  1. Open Devices view          1. Open Devices view     │
│  2. See "My Public Key:        2. See "My Public Key:   │
│     cafe0123..."                   dead5678..."          │
│  3. Click "Copy" on key        3. Click "Copy" on key   │
│                                                          │
│  4. Share B's key to A         ← (out-of-band: QR,     │
│     (paste into Admit form)       message, verbal)      │
│                                                          │
│  5. Click "Admit Device"       5. Click "Admit Device"  │
│  6. Paste B's pubkey           6. Paste A's pubkey      │
│  7. Enter label "Bob's Phone"  7. Enter label "Alice's  │
│  8. Confirm                       Laptop"               │
│                                 8. Confirm              │
│                                                          │
│  Both devices now have each other in their trust group. │
│  Next sync will propagate the full vault.               │
└─────────────────────────────────────────────────────────┘
```

Both devices need to admit each other (mutual trust). The UI should make this clear with instructional text in the Admit dialog:

> "To sync vaults between devices, both devices must admit each other.
> Share your public key with the other device, and enter their public key below."

## Data Models

### Extension `GET_ITEM` Response

```typescript
interface FullVaultItem {
  id: string;
  kind: string;
  name: string;
  username?: string;
  password?: string;
  totp_secret?: string;
  uris?: { uri: string; match: string }[];
  note?: string;
  cardholder?: string;
  card_number?: string;
  expiry?: string;
  cvv?: string;
  identity?: {
    first_name?: string;
    last_name?: string;
    address?: string;
    city?: string;
    country?: string;
    phone?: string;
    email?: string;
  };
  created_at: string;
  updated_at: string;
}
```

### Extension Device Entry

```typescript
interface DeviceEntry {
  device_id: string;
  nostr_pubkey: string;
  label: string;
  added_at: string;
  added_by: string;
  revoked: boolean;
  revoked_at?: string;
}
```

### Desktop Admit Device Input

```typescript
// Tauri command call
invoke("admit_device", { pubkeyHex: "cafe0123...", label: "Bob's Phone" })
// Returns device_id string on success
```

### Extension Message Protocol (new handlers)

```typescript
// GET_PASSWORD
{ type: "GET_PASSWORD", payload: { id: "uuid" } }
→ { password: "secret123" } | { error: "..." }

// GET_ITEM
{ type: "GET_ITEM", payload: { id: "uuid" } }
→ { item: FullVaultItem } | { error: "..." }

// LIST_DEVICES
{ type: "LIST_DEVICES" }
→ { devices: DeviceEntry[] } | { error: "..." }

// ADMIT_DEVICE
{ type: "ADMIT_DEVICE", payload: { pubkeyHex: "...", label: "..." } }
→ { success: true, deviceId: "uuid" } | { error: "..." }

// REVOKE_DEVICE
{ type: "REVOKE_DEVICE", payload: { deviceId: "uuid" } }
→ { success: true } | { error: "..." }
```

## Error Handling

| Error | Source | Handling |
|---|---|---|
| Invalid pubkey hex (wrong length/chars) | Admit dialog | Inline validation error, prevent submit |
| Device already admitted | `admit_device` | Show error: "This device is already in the trust group" |
| Cannot revoke self | `revoke_device` | Hide revoke button for current device |
| Clipboard write failure | Copy buttons | Show toast: "Failed to copy" |
| Password too short (< 4) | Generator | Return error, show inline message |
| Vault locked during operation | Background handlers | Return `{ error: "Vault is locked" }` |
| Item not found | GET_PASSWORD / GET_ITEM | Return `{ error: "Item not found" }` |

## Visual Design Notes

### Extension
- All styles remain inline (consistent with existing views)
- Dark theme tokens: bg `#16213e`, text `#e0e0e0`, button `#0f3460`, border `#444`, error `#ff6b6b`
- Popup width: 360px (fixed by manifest)
- Toast: positioned top-center, semi-transparent dark background

### Desktop
- Uses existing Tailwind + `zvault-*` color palette
- Modals use existing backdrop pattern (`fixed inset-0 bg-black/50`)
- Device management page follows same layout as VaultList (header + main content area)
- Confirmation dialogs use red accent for destructive actions
