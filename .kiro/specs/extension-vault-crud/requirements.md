# Requirements Document

## Introduction

This feature adds the ability to create new vault items directly from the ZVault browser extension popup. Users can create Login, Secure Note, Card, and Identity items through type-specific forms without leaving the extension. The feature includes a built-in password generator for Login items, auto-populates the current tab URL for convenience, and triggers an immediate Nostr sync to connected devices after saving.

## Glossary

- **Extension_Popup**: The ZVault browser extension popup UI rendered via React in the WXT extension framework.
- **Background_Worker**: The extension's background service worker that holds session state and performs vault encryption/persistence.
- **WASM_Bridge**: The WebAssembly module compiled from zvault-core that performs cryptographic and vault operations in-browser.
- **Item_Create_View**: The new scrollable form view within the Extension_Popup for creating vault items.
- **Password_Generator**: A component and corresponding WASM function that produces cryptographically random passwords with configurable parameters.
- **Sync_Engine**: The Nostr-based sync mechanism (NIP-44 encryption, NIP-59 gift-wrap) that propagates vault changes to other admitted devices.
- **Current_Tab_URL**: The URL of the browser tab that was active when the user opened the extension popup.
- **VaultItem**: A single credential entry in the vault, one of four kinds: Login, SecureNote, Card, or Identity.

## Requirements

### Requirement 1: Navigate to Item Creation

**User Story:** As a user with an unlocked vault, I want to access a form to create new items so that I can add credentials directly from the browser extension.

#### Acceptance Criteria

1. WHILE the vault is unlocked and the Item_List_View is displayed, THE Extension_Popup SHALL display a visible button labelled with a plus icon or "Add" text that navigates to the Item_Create_View.
2. WHEN the user activates the add-item button, THE Extension_Popup SHALL display the Item_Create_View with an item-type selector defaulting to "Login".
3. WHEN the user activates a back or cancel control on the Item_Create_View, THE Extension_Popup SHALL return to the Item_List_View without persisting any data.

### Requirement 2: Item Type Selection

**User Story:** As a user, I want to choose what kind of item to create so that the form shows the relevant fields for my credential type.

#### Acceptance Criteria

1. THE Item_Create_View SHALL present a type selector with exactly four options: Login, Secure Note, Card, and Identity.
2. WHEN the user selects a different item type, THE Item_Create_View SHALL replace the type-specific form fields with the fields corresponding to the newly selected type.
3. THE Item_Create_View SHALL preserve the item name field value when the user switches between item types.

### Requirement 3: Login Item Form

**User Story:** As a user, I want to fill in login credentials (name, username, password, TOTP secret, URIs) so that I can store a complete login entry.

#### Acceptance Criteria

1. WHEN the selected item type is Login, THE Item_Create_View SHALL display form fields for: name (required), username, password, TOTP secret, and at least one URI entry.
2. THE Item_Create_View SHALL display all Login fields in a single scrollable view without pagination or multi-step navigation.
3. WHEN the user adds a URI entry, THE Item_Create_View SHALL allow the user to specify both the URI value and a match strategy (Domain, Host, StartsWith, Exact, Regex, Never).
4. THE Item_Create_View SHALL allow the user to add multiple URI entries to a single Login item.

### Requirement 4: Secure Note Item Form

**User Story:** As a user, I want to create secure notes so that I can store free-form sensitive text.

#### Acceptance Criteria

1. WHEN the selected item type is Secure Note, THE Item_Create_View SHALL display form fields for: name (required) and note content (multi-line text area).
2. THE Item_Create_View SHALL render the note field as a multi-line text input with a minimum visible height of 4 text lines.

### Requirement 5: Card Item Form

**User Story:** As a user, I want to store payment card details so that I can access them securely from the extension.

#### Acceptance Criteria

1. WHEN the selected item type is Card, THE Item_Create_View SHALL display form fields for: name (required), cardholder name, card number, expiry date, and CVV.
2. THE Item_Create_View SHALL mask the CVV field input by default, displaying dots or asterisks instead of the entered characters.

### Requirement 6: Identity Item Form

**User Story:** As a user, I want to store identity information so that I can auto-fill personal details into web forms.

#### Acceptance Criteria

1. WHEN the selected item type is Identity, THE Item_Create_View SHALL display form fields for: name (required), first name, last name, address, city, country, phone, and email.

### Requirement 7: Password Generator

**User Story:** As a user creating a Login item, I want to generate a strong random password so that I do not need to invent one manually.

#### Acceptance Criteria

1. WHEN the selected item type is Login, THE Item_Create_View SHALL display a generate-password button adjacent to the password field.
2. WHEN the user activates the generate-password button, THE WASM_Bridge SHALL produce a cryptographically random password using a secure random number generator (OsRng equivalent in WASM via getrandom).
3. WHEN the WASM_Bridge returns a generated password, THE Item_Create_View SHALL populate the password field with the generated value.
4. THE WASM_Bridge SHALL generate passwords with a default length of 20 characters containing at least one uppercase letter, one lowercase letter, one digit, and one special character.
5. THE Password_Generator WASM function SHALL be exposed as `generate_password` accepting an optional length parameter and returning the generated password string.

### Requirement 8: Auto-Fill Current Tab URL

**User Story:** As a user creating a Login item, I want the current website URL pre-filled so that the new credential is automatically associated with the site I am on.

#### Acceptance Criteria

1. WHEN the Item_Create_View is opened with item type Login, THE Extension_Popup SHALL query the browser for the active tab URL.
2. WHEN the active tab URL is retrieved and uses the HTTPS scheme, THE Item_Create_View SHALL pre-populate the first URI field with that URL and set the match strategy to Domain.
3. IF the active tab URL cannot be retrieved or uses a non-HTTPS scheme (including HTTP, chrome://, about://, moz-extension://), THEN THE Item_Create_View SHALL leave the first URI field empty.

### Requirement 9: Form Validation

**User Story:** As a user, I want clear feedback when required fields are missing so that I do not accidentally save an incomplete item.

#### Acceptance Criteria

1. THE Item_Create_View SHALL require the name field to be non-empty for all item types before allowing submission.
2. WHEN the user attempts to save with the name field empty, THE Item_Create_View SHALL display a validation error message adjacent to the name field and prevent submission.
3. THE Item_Create_View SHALL disable the save button while a save operation is in progress to prevent duplicate submissions.

### Requirement 10: Save Item via Background Worker

**User Story:** As a user, I want my new item persisted securely so that it survives extension restarts and browser closures.

#### Acceptance Criteria

1. WHEN the user submits a valid item form, THE Extension_Popup SHALL send an ADD_ITEM message to the Background_Worker containing the item data as a JSON payload.
2. WHEN the Background_Worker receives an ADD_ITEM message, THE Background_Worker SHALL use the WASM_Bridge to add the item to the in-memory vault JSON, re-encrypt the vault, and persist the encrypted blob to browser.storage.local.
3. WHEN the Background_Worker successfully persists the item, THE Extension_Popup SHALL navigate back to the Item_List_View displaying the updated item list including the newly created item.
4. IF the Background_Worker returns an error (vault locked, encryption failure), THEN THE Item_Create_View SHALL display the error message to the user and retain the form data.

### Requirement 11: Auto-Sync After Save

**User Story:** As a user with multiple devices, I want new items to sync immediately so that my other devices receive the credential without manual intervention.

#### Acceptance Criteria

1. WHEN the Background_Worker successfully persists a new item, THE Background_Worker SHALL initiate a Nostr sync event by building a full sync message (NIP-44 encrypted, NIP-59 gift-wrapped) and publishing it to configured relays.
2. IF the sync operation fails (network error, no configured relays, no admitted devices), THEN THE Background_Worker SHALL log the failure and return success for the save operation without blocking the user.
3. THE Background_Worker SHALL perform the sync operation asynchronously after confirming save success to the Extension_Popup, so that the user is not blocked by network latency.

### Requirement 12: WASM Password Generator Function

**User Story:** As a developer, I want a `generate_password` function exposed from the WASM crate so that the extension can generate secure passwords without a network call.

#### Acceptance Criteria

1. THE WASM_Bridge SHALL expose a `generate_password(length: Option<u32>)` function that returns a random password string.
2. WHEN the length parameter is not provided, THE WASM_Bridge SHALL generate a password of 20 characters.
3. THE WASM_Bridge SHALL use the `getrandom` crate (WASM-compatible secure RNG) as the entropy source for password generation.
4. THE WASM_Bridge SHALL guarantee the generated password contains at least one character from each of the four character classes: uppercase ASCII letters (A-Z), lowercase ASCII letters (a-z), ASCII digits (0-9), and special characters from the set `!@#$%^&*()_+-=[]{}|;:,.<>?`.
5. IF the requested length is less than 4, THEN THE WASM_Bridge SHALL return an error indicating the minimum length is 4.

### Requirement 13: Accessibility

**User Story:** As a user relying on assistive technology, I want the item creation form to be accessible so that I can create items using a screen reader or keyboard navigation.

#### Acceptance Criteria

1. THE Item_Create_View SHALL associate every form input with a visible label element using the `htmlFor`/`id` attribute pairing.
2. THE Item_Create_View SHALL support full keyboard navigation: Tab to move between fields, Enter or a dedicated button to submit, Escape to cancel.
3. WHEN a validation error is displayed, THE Item_Create_View SHALL mark the error element with `role="alert"` so assistive technologies announce the error.
4. THE Item_Create_View SHALL use appropriate `aria-label` or `aria-describedby` attributes on icon-only buttons (generate password, add URI, remove URI).

### Requirement 14: Visual Consistency

**User Story:** As a user, I want the item creation form to match the existing extension visual style so that the experience feels cohesive.

#### Acceptance Criteria

1. THE Item_Create_View SHALL use inline styles consistent with the existing dark theme: background color #16213e, text color #e0e0e0, button background #0f3460, border color #444, and error text color #ff6b6b.
2. THE Item_Create_View SHALL use the same input field styling (padding, border-radius, font-size) as the existing UnlockView and CreateVaultView forms.
3. THE Item_Create_View SHALL render within the extension popup dimensions (width 360px) with vertical scrolling enabled for overflow content.
