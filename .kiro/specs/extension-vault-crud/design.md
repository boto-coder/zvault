# Design Document: Extension Vault CRUD

## Architecture Overview

This feature adds an Item_Create_View to the existing browser extension popup, allowing users to create vault items (Login, Secure Note, Card, Identity) directly from the extension. The architecture follows the existing pattern: React popup → `browser.runtime.sendMessage` → background service worker → WASM bridge → `browser.storage.local`.

A new `generate_password` function is added to the `zvault-wasm` crate to provide cryptographically secure password generation without network calls.

```
┌─────────────────────────────────────┐
│       Extension Popup (React)       │
│                                     │
│  ItemListView ──► ItemCreateView    │
│    [+ Add]         [Form Fields]    │
│                    [Generate PW]    │
│                    [Save / Cancel]  │
└───────────────┬─────────────────────┘
                │ browser.runtime.sendMessage
                ▼
┌─────────────────────────────────────┐
│     Background Service Worker       │
│                                     │
│  ADD_ITEM handler                   │
│    → WASM add_item (update JSON)    │
│    → WASM encrypt_vault             │
│    → browser.storage.local.set      │
│    → fire-and-forget Nostr sync     │
└───────────────┬─────────────────────┘
                │
                ▼
┌─────────────────────────────────────┐
│   zvault-wasm (WebAssembly)         │
│                                     │
│  add_item(vault_json, item_json)    │
│  encrypt_vault(password, json)      │
│  generate_password(length?)         │
└─────────────────────────────────────┘
```

## Components

### 1. ItemCreateView (React Component)

**Location:** `apps/extension/src/entrypoints/popup/App.tsx` (new component in the existing file)

A single scrollable form view rendered inside the popup when the user navigates from the item list. Manages:
- Item type selection (dropdown, defaults to "Login")
- Type-specific form fields (conditional rendering)
- Password generation trigger
- Current-tab URL auto-population
- Form validation
- Save submission and error display

**State management:** Local `useState` hooks — no external state library needed. The view is ephemeral; navigation away discards state.

### 2. View Router Update

**Location:** `apps/extension/src/entrypoints/popup/App.tsx`

The existing `View` type union gains a new member `"create-item"`. The `App` component routes to `ItemCreateView` when in this state. `ItemListView` gains an "Add" button that triggers `setView("create-item")`.

### 3. generate_password WASM Function

**Location:** `crates/zvault-wasm/src/lib.rs`

New `#[wasm_bindgen]` function:

```rust
#[wasm_bindgen]
pub fn generate_password(length: Option<u32>) -> Result<String, JsValue> {
    let len = length.unwrap_or(20) as usize;
    if len < 4 {
        return Err(JsValue::from_str(
            "minimum password length is 4",
        ));
    }

    use getrandom::getrandom;

    const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
    const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    const DIGITS: &[u8] = b"0123456789";
    const SPECIAL: &[u8] = b"!@#$%^&*()_+-=[]{}|;:,.<>?";
    const ALL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789!@#$%^&*()_+-=[]{}|;:,.<>?";

    let mut password = Vec::with_capacity(len);
    let mut random_bytes = vec![0u8; len + 4]; // extra for guaranteed slots
    getrandom(&mut random_bytes)
        .map_err(|e| JsValue::from_str(&format!("RNG error: {e}")))?;

    // Guarantee one from each class in the first 4 positions
    password.push(UPPER[(random_bytes[0] as usize) % UPPER.len()]);
    password.push(LOWER[(random_bytes[1] as usize) % LOWER.len()]);
    password.push(DIGITS[(random_bytes[2] as usize) % DIGITS.len()]);
    password.push(SPECIAL[(random_bytes[3] as usize) % SPECIAL.len()]);

    // Fill remaining positions from the full character set
    for i in 4..len {
        password.push(ALL[(random_bytes[i] as usize) % ALL.len()]);
    }

    // Fisher-Yates shuffle using remaining random bytes
    let mut shuffle_bytes = vec![0u8; len];
    getrandom(&mut shuffle_bytes)
        .map_err(|e| JsValue::from_str(&format!("RNG error: {e}")))?;
    for i in (1..len).rev() {
        let j = (shuffle_bytes[i] as usize) % (i + 1);
        password.swap(i, j);
    }

    Ok(String::from_utf8(password).expect("all chars are ASCII"))
}
```

**Entropy source:** `getrandom` crate with the `js` feature — delegates to `crypto.getRandomValues()` in the browser, which is cryptographically secure.

### 4. WASM Bridge Update

**Location:** `apps/extension/src/lib/wasm.ts`

Add `generate_password` to the `ZVaultWasm` interface and wire it through `initWasm()`:

```typescript
export interface ZVaultWasm {
  // ... existing methods ...
  generate_password(length?: number): string;
}
```

### 5. Background Worker Sync Extension

**Location:** `apps/extension/src/entrypoints/background.ts`

After the existing `ADD_ITEM` handler persists the encrypted vault, it fires an async (non-blocking) Nostr sync:

```typescript
case "ADD_ITEM": {
  // ... existing persist logic ...
  // Fire-and-forget sync
  triggerNostrSync(sessionVaultJson).catch((err) =>
    console.warn("[zvault] sync failed:", err)
  );
  return { success: true };
}
```

The `triggerNostrSync` helper builds the NIP-44/NIP-59 message and publishes to configured relays. Failure is logged but never propagated to the popup — the save is already confirmed.

## Interfaces

### Message Protocol (Popup → Background)

**ADD_ITEM** message payload — the JSON structure sent from the popup to the background:

```typescript
interface AddItemPayload {
  kind: "login" | "secure_note" | "card" | "identity";
  name: string;

  // Login fields
  username?: string;
  password?: string;
  totp_secret?: string;
  uris?: Array<{ uri: string; match: UriMatch }>;

  // Secure Note fields
  note?: string;

  // Card fields
  cardholder?: string;
  card_number?: string;
  expiry?: string;
  cvv?: string;

  // Identity fields
  identity?: {
    first_name?: string;
    last_name?: string;
    address?: string;
    city?: string;
    country?: string;
    phone?: string;
    email?: string;
  };
}

type UriMatch = "domain" | "host" | "starts_with" | "exact" | "regex" | "never";
```

**Response:**
```typescript
{ success: true }
| { error: string }
```

### WASM Bridge Interface (TypeScript)

```typescript
export interface ZVaultWasm {
  create_vault(password: string): Uint8Array;
  open_vault(password: string, data: Uint8Array): string;
  encrypt_vault(password: string, vault_json: string): Uint8Array;
  add_item(vault_json: string, item_json: string): string;
  list_items(vault_json: string): unknown[];
  generate_totp(secret: string): string;
  generate_password(length?: number): string;  // NEW
}
```

### ItemCreateView Props

```typescript
interface ItemCreateViewProps {
  onSave: () => void;     // callback after successful save — navigates to item list
  onCancel: () => void;   // callback to return to item list without saving
}
```

### Current Tab URL Query

```typescript
async function getCurrentTabUrl(): Promise<string | null> {
  try {
    const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
    if (tab?.url && tab.url.startsWith("https://")) {
      return tab.url;
    }
  } catch {
    // Permission denied or no active tab
  }
  return null;
}
```

## Data Models

### Form State (TypeScript)

```typescript
interface CreateItemFormState {
  kind: ItemKind;
  name: string;

  // Login
  username: string;
  password: string;
  totpSecret: string;
  uris: Array<{ uri: string; match: UriMatch }>;

  // Secure Note
  note: string;

  // Card
  cardholder: string;
  cardNumber: string;
  expiry: string;
  cvv: string;

  // Identity
  firstName: string;
  lastName: string;
  address: string;
  city: string;
  country: string;
  phone: string;
  email: string;
}

type ItemKind = "login" | "secure_note" | "card" | "identity";
```

### Mapping: Form State → ADD_ITEM Payload

Only the fields relevant to the selected `kind` are included in the message payload. Empty optional strings are omitted (not sent as `""`).

```typescript
function buildPayload(form: CreateItemFormState): AddItemPayload {
  const base = { kind: form.kind, name: form.name.trim() };

  switch (form.kind) {
    case "login":
      return {
        ...base,
        username: form.username || undefined,
        password: form.password || undefined,
        totp_secret: form.totpSecret || undefined,
        uris: form.uris.filter(u => u.uri.trim() !== ""),
      };
    case "secure_note":
      return { ...base, note: form.note || undefined };
    case "card":
      return {
        ...base,
        cardholder: form.cardholder || undefined,
        card_number: form.cardNumber || undefined,
        expiry: form.expiry || undefined,
        cvv: form.cvv || undefined,
      };
    case "identity":
      return {
        ...base,
        identity: {
          first_name: form.firstName || undefined,
          last_name: form.lastName || undefined,
          address: form.address || undefined,
          city: form.city || undefined,
          country: form.country || undefined,
          phone: form.phone || undefined,
          email: form.email || undefined,
        },
      };
  }
}
```

### Rust Data Model (existing — no changes)

The `VaultItem` struct in `zvault-core` already supports all four item kinds with optional fields. The WASM `add_item` function deserialises the JSON payload into `VaultItem` using serde — no new Rust types are needed.

## Error Handling

| Error Condition | Source | Handling |
|---|---|---|
| Name field empty | Popup validation | Display inline error, prevent submission |
| Password length < 4 | WASM `generate_password` | Return `Err(JsValue)` → popup shows toast/inline error |
| Vault locked during save | Background worker | Return `{ error: "Vault is locked" }` → popup displays error, retains form |
| Encryption failure | WASM `encrypt_vault` | Return `{ error: ... }` → popup displays error, retains form |
| Invalid item JSON | WASM `add_item` | Return `{ error: ... }` → popup displays error, retains form |
| Tab URL query failure | Popup `browser.tabs.query` | Silently return `null` → leave URI field empty |
| Non-HTTPS tab URL | Popup logic | Treat as no URL → leave URI field empty |
| Nostr sync failure | Background worker | `console.warn` log; save result already confirmed to popup |
| `getrandom` failure (WASM) | `generate_password` | Return `Err(JsValue)` with "RNG error" message |

### Error Display Strategy

- **Validation errors** (name empty): Inline red text (`#ff6b6b`) adjacent to the field, with `role="alert"`.
- **Backend errors** (vault locked, encryption failure): Banner-style error at the top of the form, with `role="alert"`.
- **Non-blocking failures** (sync, tab URL): No user-visible error. Logged to console.

## Keyboard Navigation & Accessibility

- All inputs have paired `<label htmlFor="...">` and `id` attributes.
- Tab order follows visual top-to-bottom layout (no explicit `tabIndex` overrides needed).
- `Escape` key handler on the form container triggers cancel/back navigation.
- `Enter` on the form's submit button (or pressing Enter in the last field with form `onSubmit`) triggers save.
- Icon-only buttons ("Generate password", "Add URI", "Remove URI") carry `aria-label` attributes.
- Validation and backend errors use `role="alert"` for screen reader announcement.

## Visual Design

All styles are inline (consistent with existing views). Key tokens:

| Token | Value | Usage |
|---|---|---|
| Background | `#16213e` | Input fields, form background |
| Text | `#e0e0e0` | Labels, input text, buttons |
| Button bg | `#0f3460` | Primary action buttons |
| Border | `#444` | Input borders, section dividers |
| Error text | `#ff6b6b` | Validation error messages |
| Popup width | `360px` | Fixed by manifest; form must fit |

Input field shared styles (matching CreateVaultView):
```typescript
const inputStyle: React.CSSProperties = {
  width: "100%",
  padding: "0.6rem",
  borderRadius: "4px",
  border: "1px solid #444",
  background: "#16213e",
  color: "#e0e0e0",
  fontSize: "1rem",
  marginBottom: "0.75rem",
};
```

The form container has `overflowY: "auto"` with a max height matching the popup viewport to enable scrolling for longer forms (Identity, Login with multiple URIs).

## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system — essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

<!--
Acceptance Criteria Testing Prework:

1.1 WHILE the vault is unlocked and the Item_List_View is displayed, THE Extension_Popup SHALL display a visible button labelled with a plus icon or "Add" text that navigates to the Item_Create_View.
  Thoughts: This is a UI rendering requirement — when a specific state is active, a button must be visible. It's testable as an example-based rendering test (given unlocked state, does the button render?).
  Classification: EXAMPLE
  Test Strategy: Render ItemListView and assert the add button is present.

1.2 WHEN the user activates the add-item button, THE Extension_Popup SHALL display the Item_Create_View with an item-type selector defaulting to "Login".
  Thoughts: Specific navigation behavior. Testable as example — click button, assert correct view with correct default.
  Classification: EXAMPLE
  Test Strategy: Simulate button click, verify view transition and default type.

1.3 WHEN the user activates a back or cancel control on the Item_Create_View, THE Extension_Popup SHALL return to the Item_List_View without persisting any data.
  Thoughts: This is a universal requirement — for any form state, cancelling should discard everything and navigate back. We could generate random form data, cancel, and verify nothing was persisted.
  Classification: PROPERTY
  Test Strategy: Fill random form data, trigger cancel, verify no message sent and view returns to list.

2.1 THE Item_Create_View SHALL present a type selector with exactly four options: Login, Secure Note, Card, and Identity.
  Thoughts: Structural UI assertion — exactly 4 options must exist. Example-based.
  Classification: EXAMPLE
  Test Strategy: Render component, assert 4 options with correct labels.

2.2 WHEN the user selects a different item type, THE Item_Create_View SHALL replace the type-specific form fields with the fields corresponding to the newly selected type.
  Thoughts: For any pair of item types (source, target), switching should show the target's fields. 4×4 = 16 combinations — could be property-based.
  Classification: PROPERTY
  Test Strategy: For any item type, switching to that type should render exactly the fields defined for that type.

2.3 THE Item_Create_View SHALL preserve the item name field value when the user switches between item types.
  Thoughts: For any name string and any pair of types, switching should preserve the name. This is a universal property.
  Classification: PROPERTY
  Test Strategy: Set a random name, switch to a random type, verify name is preserved.

3.3 WHEN the user adds a URI entry, THE Item_Create_View SHALL allow the user to specify both the URI value and a match strategy (Domain, Host, StartsWith, Exact, Regex, Never).
  Thoughts: UI rendering check for a specific interaction. Example-based.
  Classification: EXAMPLE
  Test Strategy: Add URI entry, verify URI input and match strategy selector render.

3.4 THE Item_Create_View SHALL allow the user to add multiple URI entries to a single Login item.
  Thoughts: For any number N of URI additions, the form should show N URI fields. This is a growth property.
  Classification: PROPERTY
  Test Strategy: Add N URI entries (random N), verify N URI fields are visible.

7.4 THE WASM_Bridge SHALL generate passwords with a default length of 20 characters containing at least one uppercase letter, one lowercase letter, one digit, and one special character.
  Thoughts: This is a universal property: for any call to generate_password without a length, the result must be 20 chars and contain all 4 character classes. Pure function, perfect for PBT.
  Classification: PROPERTY
  Test Strategy: Call generate_password() many times, verify length=20 and all 4 classes present.

8.2 WHEN the active tab URL is retrieved and uses the HTTPS scheme, THE Item_Create_View SHALL pre-populate the first URI field with that URL and set the match strategy to Domain.
  Thoughts: For any HTTPS URL, the form should auto-populate. Universal over all HTTPS URLs.
  Classification: PROPERTY
  Test Strategy: Generate random HTTPS URLs, verify first URI field is populated with that URL and match=Domain.

8.3 IF the active tab URL cannot be retrieved or uses a non-HTTPS scheme (including HTTP, chrome://, about://, moz-extension://), THEN THE Item_Create_View SHALL leave the first URI field empty.
  Thoughts: For any non-HTTPS URL or error condition, the URI field must be empty. Universal over non-HTTPS schemes.
  Classification: PROPERTY
  Test Strategy: Generate random non-HTTPS URLs, verify first URI field remains empty.

9.1 THE Item_Create_View SHALL require the name field to be non-empty for all item types before allowing submission.
  Thoughts: For any item type and any empty/whitespace name, submission must be prevented. Universal property.
  Classification: PROPERTY
  Test Strategy: For any item type and any whitespace-only name string, verify save is blocked.

9.3 THE Item_Create_View SHALL disable the save button while a save operation is in progress to prevent duplicate submissions.
  Thoughts: During async operation, button must be disabled. Example-based (specific timing scenario).
  Classification: EXAMPLE
  Test Strategy: Trigger save, assert button is disabled during pending state.

10.1 WHEN the user submits a valid item form, THE Extension_Popup SHALL send an ADD_ITEM message to the Background_Worker containing the item data as a JSON payload.
  Thoughts: For any valid form state across all item types, submitting must produce a correctly-shaped ADD_ITEM message. Universal.
  Classification: PROPERTY
  Test Strategy: Generate random valid form data for each type, submit, verify message shape matches payload interface.

10.4 IF the Background_Worker returns an error (vault locked, encryption failure), THEN THE Item_Create_View SHALL display the error message to the user and retain the form data.
  Thoughts: For any error string, the form must show it and retain data. Universal over error messages.
  Classification: PROPERTY
  Test Strategy: Return various error strings from background, verify form displays error and preserves state.

11.2 IF the sync operation fails (network error, no configured relays, no admitted devices), THEN THE Background_Worker SHALL log the failure and return success for the save operation without blocking the user.
  Thoughts: For any sync failure mode, the save response must still be success. Universal over error types.
  Classification: PROPERTY
  Test Strategy: Mock sync to fail with various errors, verify ADD_ITEM still returns success.

12.1-12.5 (generate_password specification)
  12.1 Thoughts: Exposure check — example-based (function exists on interface).
  Classification: EXAMPLE
  12.2 Thoughts: For any call without length param, output must be 20 chars. Universal property (same as 7.4).
  Classification: PROPERTY (redundant with 7.4 — merge)
  12.3 Thoughts: Implementation detail about entropy source. Not testable as a property.
  Classification: SMOKE
  12.4 Thoughts: For any generated password of any valid length, all 4 character classes must be present. Universal property.
  Classification: PROPERTY
  12.5 Thoughts: For any length < 4, an error must be returned. Universal over invalid lengths.
  Classification: PROPERTY
  Test Strategy: Generate passwords with random lengths < 4, verify error. Generate passwords with random valid lengths, verify all 4 classes.

13.1-13.4 (Accessibility)
  Thoughts: These are structural rendering requirements. They hold for all form states but are best tested via snapshot/example (checking DOM structure). Not amenable to property-based randomized testing since there's no meaningful input variation.
  Classification: EXAMPLE
  Test Strategy: Render each form type, assert label/id pairings, aria attributes, role="alert" on errors.

14.1-14.3 (Visual consistency)
  Thoughts: Style assertions. Example-based — check computed styles on rendered elements.
  Classification: EXAMPLE
  Test Strategy: Render form, assert specific style values on elements.

Property Reflection:
- 7.4 and 12.2 are the same requirement (default length 20, all 4 classes) → merge into one property.
- 12.4 generalises 7.4/12.2 (any valid length, all 4 classes) → 12.4 subsumes both. Keep one property covering all valid lengths.
- 1.3 (cancel discards) and 2.3 (name preserved on type switch) are distinct concerns — keep both.
- 8.2 and 8.3 are complementary (HTTPS → populate, non-HTTPS → empty). Could combine into one property: "URI auto-fill correctness based on URL scheme." Keep as one.
- 9.1 (name required for submission) is distinct from 10.1 (valid form produces correct payload).
- 10.4 (error display with form retention) and 11.2 (sync failure doesn't block save) are independent.
-->

### Property 1: Cancel discards all form state

*For any* item type and *for any* combination of filled-in form fields, when the user activates the cancel/back control, no `ADD_ITEM` message is sent to the background worker and the view returns to the item list.

**Validates: Requirements 1.3**

### Property 2: Type switching preserves the name field

*For any* non-empty name string and *for any* pair of item types (source type, target type), switching the type selector from source to target preserves the name field value unchanged.

**Validates: Requirements 2.3**

### Property 3: Type switching renders correct fields

*For any* selected item type, the rendered form fields correspond exactly to the specification for that type: Login shows username/password/TOTP/URIs, Secure Note shows note textarea, Card shows cardholder/number/expiry/CVV, Identity shows first name/last name/address/city/country/phone/email.

**Validates: Requirements 2.2, 3.1, 4.1, 5.1, 6.1**

### Property 4: URI list grows monotonically on add

*For any* Login form state with N URI entries, adding a new URI entry results in exactly N+1 URI entries visible in the form.

**Validates: Requirements 3.4**

### Property 5: Password generation guarantees character class coverage

*For any* valid length L (where L ≥ 4), calling `generate_password(L)` returns a string of exactly L characters that contains at least one uppercase letter (A-Z), at least one lowercase letter (a-z), at least one digit (0-9), and at least one special character from `!@#$%^&*()_+-=[]{}|;:,.<>?`.

**Validates: Requirements 7.4, 12.2, 12.4**

### Property 6: Password generation rejects invalid lengths

*For any* length L where L < 4, calling `generate_password(L)` returns an error indicating the minimum length is 4.

**Validates: Requirements 12.5**

### Property 7: URL scheme determines auto-fill behavior

*For any* URL string, if the URL starts with `https://` then the first URI field is pre-populated with that URL and match strategy set to "Domain"; otherwise (HTTP, chrome://, about://, moz-extension://, or retrieval failure), the first URI field is left empty.

**Validates: Requirements 8.2, 8.3**

### Property 8: Empty name prevents submission for all item types

*For any* item type and *for any* name value that is empty or consists entirely of whitespace, the form submission is prevented and no `ADD_ITEM` message is sent.

**Validates: Requirements 9.1, 9.2**

### Property 9: Valid form produces correct ADD_ITEM payload

*For any* valid form state (non-empty name, any item type with arbitrary field values), submitting the form sends an `ADD_ITEM` message whose payload includes the correct `kind`, `name`, and only the fields relevant to the selected item type with empty optional fields omitted.

**Validates: Requirements 10.1**

### Property 10: Backend errors are displayed without losing form data

*For any* error string returned by the background worker in response to an `ADD_ITEM` message, the Item_Create_View displays the error message and all form field values remain unchanged.

**Validates: Requirements 10.4**

### Property 11: Sync failure does not affect save success

*For any* sync failure condition (network error, no relays, no admitted devices), the `ADD_ITEM` handler still returns `{ success: true }` to the popup after the vault is persisted.

**Validates: Requirements 11.2, 11.3**
