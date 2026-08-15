# Design: Bugfix — Extension Save Item + Device Identity Not Found

## Bug 1: AddItemInput pattern

The frontend sends a partial item payload:
```json
{
  "kind": "login",
  "name": "My Login",
  "username": "user@example.com",
  "password": "s3cr3t",
  "uris": [{"uri": "https://example.com", "match_type": "Domain"}]
}
```

The WASM `add_item` function currently tries to deserialize this as a full `VaultItem`, which requires `id`, `created_at`, and `updated_at`. 

### Solution

Introduce `AddItemInput` (deserialize-only struct) that mirrors the frontend payload shape:

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddItemInput {
    kind: ItemKind,
    name: String,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    totp_secret: Option<String>,
    #[serde(default)]
    uris: Option<Vec<UriInput>>,
    #[serde(default)]
    note: Option<String>,
    // ... card fields, identity fields
    #[serde(default)]
    favourite: bool,
}
```

Then construct:
```rust
let item = VaultItem {
    id: Uuid::new_v4(),
    kind: input.kind,
    name: input.name,
    username: input.username,
    password: input.password,
    // ...
    created_at: Utc::now(),
    updated_at: Utc::now(),
};
vault.add_item(item);
```

This matches how the CLI's `--json` flag works (`JsonItemInput` → `VaultItem`).

## Bug 2: Auto-initialize device identity

### Current flow (broken)
```
User opens vault → clicks "Invite Device" → error: no device identity
```

### Fixed flow
```
User opens vault → clicks "Invite Device" → system detects no device
→ auto-generates keypair → adds to vault.devices → proceeds with invite
```

### Security considerations
- The auto-generated keypair must be stored securely (OS keychain on desktop, encrypted in extension storage)
- The device label should be descriptive but not leak sensitive info (hostname is acceptable)
- Auto-init must not overwrite an existing device identity
- The auto-init operation must save the vault (persist the new device entry)
