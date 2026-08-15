# Bug Report: Extension Save Item Fails + Device Identity Not Found in Pairing

## Bug 1: Saving item in Firefox extension gives "missing field `id`"

**Severity:** Critical — users cannot save new items in the browser extension

**Observed behaviour:**
When submitting the "Add Item" form in the Firefox extension popup, the save fails with:
```
invalid item JSON: missing field `id` at line 1 column 91
```

**Expected behaviour:**
New items should be created successfully without the user needing to supply an `id` — the backend should generate a fresh UUID.

**Root cause:**
The WASM `add_item(vault_json, item_json)` function in `crates/zvault-wasm/src/lib.rs` deserializes `item_json` directly into a `VaultItem` struct via `serde_json::from_str::<VaultItem>(item_json)`. The `VaultItem` struct requires an `id: Uuid` field, but the frontend sends a payload without `id` (expecting the backend to generate one).

**File:** `crates/zvault-wasm/src/lib.rs`, `add_item` function (line ~85)

**Fix approach:**
Create an intermediate `AddItemInput` struct (without `id`, `created_at`, `updated_at`) that matches what the frontend sends. Deserialize into `AddItemInput`, then construct a full `VaultItem` with a fresh `Uuid::new_v4()` and current timestamps.

---

## Bug 2: Invite/Join-Request returns "No active device identity found"

**Severity:** High — device pairing is unusable without manual CLI setup

**Observed behaviour:**
In both the Firefox extension and desktop Tauri app, clicking "Invite Device" or "Request to Join" returns:
```
No active device identity found
```

**Expected behaviour:**
The pairing flow should either:
1. Auto-initialize a device identity if none exists, or
2. Prompt the user to initialize their device identity before proceeding

**Root cause:**
The `create_invite_code` and `create_join_request_code` commands look for the first non-revoked entry in `vault.devices`. But when a vault is freshly created (via the extension or desktop app), no device identity is generated — `vault.devices` is empty. The device initialization step (`device init` in CLI, or the "Initialise Device" button in the Devices view) must be performed first, but the Invite/Join Request UI doesn't check for this or guide the user.

**Files:**
- `apps/desktop/src-tauri/src/main.rs` — `create_invite_code`, `create_join_request_code`
- `apps/extension/src/entrypoints/background.ts` — `CREATE_INVITE_CODE`, `CREATE_JOIN_REQUEST_CODE`

**Fix approach:**
Auto-initialize a device identity when the pairing flow detects `vault.devices` is empty:
- Desktop: Call the existing `init_device` logic internally before creating the code
- Extension: Call the existing `INIT_DEVICE` handler, then retry the pairing code generation
- Both should use a default label derived from the platform (e.g. "Desktop", "Firefox Extension")
- Alternatively, show a one-time dialog asking for a device label before proceeding
