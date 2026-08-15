# Requirements Document

## Introduction

This feature delivers a UX audit and improvement pass for the Firefox browser extension and the Tauri desktop app. The extension is currently minimal (flat list, placeholder copy, no detail view, no search, no device management). The desktop app is functional but missing common patterns (full add form, password generator, keyboard shortcuts, device management UI, delete confirmation). This spec also adds the device invite/admit flow as a first-class UI element to both platforms, enabling users to manage their trust group without resorting to the CLI.

## Glossary

- **Item Detail View (Extension)**: A new view in the extension popup showing full item fields with copy/reveal controls.
- **Suggested Items**: Items whose URIs match the current browser tab's domain, shown at the top of the list.
- **Toast Notification**: A brief, auto-dismissing message shown in the popup to confirm actions.
- **Device Management View**: A UI surface for viewing admitted devices, admitting new devices, and revoking existing ones.
- **Invite Flow**: The process of one device sharing its public key with another device, which then admits it to the vault's trust group.
- **Trust Group**: The set of devices that are admitted to a vault and can exchange sync messages.

## Requirements

### Requirement 1: Extension — Fix Copy Password

**User Story:** As an extension user, I want the copy-password button to actually copy the real password, not a placeholder string.

#### Acceptance Criteria

1. WHEN the user clicks the copy-password button on a list item, THE extension SHALL request the actual password from the background worker and copy it to the clipboard.
2. THE background worker SHALL expose a `GET_PASSWORD` message handler that returns the decrypted password for a given item ID.
3. AFTER copying, THE extension SHALL clear the clipboard after 30 seconds.
4. AFTER copying, THE extension SHALL display a "Copied!" toast notification for 2 seconds.

### Requirement 2: Extension — Search/Filter

**User Story:** As an extension user with many items, I want to search my vault quickly by typing a name or username.

#### Acceptance Criteria

1. THE Item List view SHALL display a search input at the top, above the item list.
2. THE search SHALL filter items in real-time (as-you-type) by matching against item name and username (case-insensitive substring).
3. WHEN the filter matches zero items, THE list SHALL display "No items match your search."
4. WHEN the popup opens with the vault unlocked, THE search input SHALL be auto-focused for immediate typing.

### Requirement 3: Extension — Item Detail View

**User Story:** As an extension user, I want to tap on an item to see its full details (password, notes, card info, TOTP) without opening the desktop app.

#### Acceptance Criteria

1. WHEN the user clicks on an item in the list, THE extension SHALL navigate to an Item Detail View.
2. THE Item Detail View SHALL display all fields relevant to the item type (login: username, password, URIs, TOTP; note: content; card: all card fields; identity: all identity fields).
3. Password and CVV fields SHALL be masked by default with a "Show/Hide" toggle.
4. THE Item Detail View SHALL include "Copy" buttons next to username, password, and TOTP code fields.
5. THE Item Detail View SHALL include a "Back" button to return to the item list.
6. IF the item is a login with TOTP, THE detail view SHALL show the live TOTP code and countdown (per TOTP spec).

### Requirement 4: Extension — Item Kind Icons and Visual Hierarchy

**User Story:** As an extension user, I want to quickly identify item types visually in the list.

#### Acceptance Criteria

1. EACH item in the list SHALL display a type icon: 🔑 Login, 📝 Secure Note, 💳 Card, 👤 Identity.
2. Login items SHALL show the first URI domain as a subtitle (below the name).
3. Items with TOTP SHALL display a small clock indicator (🕐 or similar).

### Requirement 5: Extension — Suggested Items for Current Site

**User Story:** As an extension user on a website, I want to see matching credentials at the top of the list so I don't have to search.

#### Acceptance Criteria

1. WHEN the popup opens with the vault unlocked, THE extension SHALL query the active tab URL.
2. IF the active tab uses HTTPS, THE extension SHALL filter items whose URIs match the current domain and display them in a "Suggested" section at the top of the list.
3. THE remaining items SHALL be displayed in an "All Items" section below.
4. IF no items match the current site, THE "Suggested" section SHALL not be shown.

### Requirement 6: Extension — Toast Notification System

**User Story:** As an extension user, I want brief visual feedback when actions succeed so that I know they worked.

#### Acceptance Criteria

1. THE extension SHALL display a toast notification component that auto-dismisses after 2 seconds.
2. Toasts SHALL be shown for: clipboard copy, item save, errors.
3. Toast messages SHALL use `role="status"` for screen reader accessibility.
4. Multiple toasts SHALL not overlap — a new toast replaces the previous one.

### Requirement 7: Desktop — Full Add Item Form

**User Story:** As a desktop user, I want to fill in all item fields (username, password, TOTP, URIs, etc.) when creating an item, not just name and type.

#### Acceptance Criteria

1. THE Add Item modal SHALL display all fields relevant to the selected item type.
2. FOR Login items: name (required), username, password (with generate button), TOTP secret, and URI list.
3. FOR Secure Note items: name (required), note textarea.
4. FOR Card items: name (required), cardholder, card number, expiry, CVV.
5. FOR Identity items: name (required), first name, last name, email, phone, address, city, country.
6. THE form SHALL validate that name is non-empty before allowing submission.
7. THE form SHALL validate TOTP secret format (if provided) before saving.

### Requirement 8: Desktop — Password Generator

**User Story:** As a desktop user, I want to generate strong random passwords in the add/edit form so I don't have to invent them manually.

#### Acceptance Criteria

1. THE desktop app SHALL expose a `generate_password` Tauri command with configurable length (default 20).
2. THE Add Item and Edit Item forms SHALL include a "Generate" button next to the password field (for Login items).
3. WHEN the generate button is clicked, THE form SHALL populate the password field with a randomly generated password (20 chars, all 4 character classes).
4. THE generated password SHALL be displayed in cleartext in the field (since the user just generated it and needs to see it).
5. THE password generator SHALL guarantee at least one character from each class: uppercase, lowercase, digit, special.

### Requirement 9: Desktop — Delete Confirmation Dialog

**User Story:** As a desktop user, I want a confirmation dialog before deleting an item so that I don't accidentally lose credentials.

#### Acceptance Criteria

1. WHEN the user clicks the delete button on an item, THE desktop app SHALL show a confirmation modal.
2. THE modal SHALL display: "Delete [item name]? This action cannot be undone."
3. THE modal SHALL have "Cancel" and "Delete" buttons. "Delete" SHALL use a red/danger colour.
4. ONLY when the user clicks "Delete" in the confirmation modal SHALL the item be deleted.

### Requirement 10: Desktop — Keyboard Shortcuts

**User Story:** As a power user, I want keyboard shortcuts for common actions so that I can work faster without reaching for the mouse.

#### Acceptance Criteria

1. `Ctrl+L` (or `Cmd+L` on macOS) SHALL lock the vault.
2. `Ctrl+N` (or `Cmd+N`) SHALL open the Add Item form.
3. `Ctrl+F` (or `Cmd+F`) SHALL focus the search input.
4. `Escape` SHALL close any open modal or navigate back from detail view.
5. Shortcuts SHALL only be active when the vault is unlocked.

### Requirement 11: Desktop — Device Management View

**User Story:** As a desktop user, I want to see which devices are in my trust group and admit or revoke devices from the desktop app, so that I don't have to use the CLI for device management.

#### Acceptance Criteria

1. THE desktop app SHALL include a "Devices" navigation option accessible from the vault list view (e.g., header button or sidebar link).
2. THE Devices view SHALL list all devices in the vault with: label, public key (truncated), admitted date, and revoked status.
3. THE current device (if identifiable) SHALL be marked with "(this device)" label.
4. THE Devices view SHALL include an "Admit Device" button that opens a dialog.
5. THE Admit Device dialog SHALL accept a public key (hex) and a device label.
6. THE Admit Device dialog SHALL include instructional text explaining the mutual-admit invite flow (both devices must admit each other).
7. THE Devices view SHALL include a "Revoke" button on each non-current device.
8. WHEN the user clicks "Revoke", THE app SHALL show a confirmation dialog explaining the consequences: "Revoke [device label]? This device will no longer receive vault updates and its messages will be rejected."
9. AFTER admitting or revoking, THE device list SHALL refresh immediately.

### Requirement 12: Extension — Device Management View

**User Story:** As an extension user, I want to view and manage my trusted devices from the extension so that I can admit a new device without opening the desktop app.

#### Acceptance Criteria

1. THE extension popup SHALL include a "Devices" navigation option (icon or menu item) accessible from the item list header.
2. THE Devices view SHALL list all admitted devices with label and truncated public key.
3. THE Devices view SHALL include an "Admit Device" button that opens a form.
4. THE Admit Device form SHALL accept a public key (hex) and a device label.
5. THE Admit Device form SHALL include instructional text explaining the mutual-admit invite flow.
6. THE Devices view SHALL include a "Revoke" button on each device (except current).
7. Revocation SHALL require confirmation before proceeding.
8. THE extension SHALL send `ADMIT_DEVICE` and `REVOKE_DEVICE` messages to the background worker for persistence.

### Requirement 13: Desktop — Show This Device's Public Key

**User Story:** As a desktop user, I want to easily see and copy my device's public key so that I can share it with another device for admission.

#### Acceptance Criteria

1. THE Devices view SHALL prominently display the current device's public key with a "Copy" button.
2. THE public key SHALL be displayed in full hex (64 characters) in a monospace font, selectable.
3. WHEN the copy button is clicked, THE public key SHALL be copied to the clipboard with a "Copied!" confirmation.
4. IF the device identity is not yet initialised, THE Devices view SHALL show an "Initialise Device" button that generates a keypair and saves it to the vault.

### Requirement 14: Extension — Show This Device's Public Key

**User Story:** As an extension user, I want to see and copy my device's public key so I can share it with the desktop app or another device for admission.

#### Acceptance Criteria

1. THE extension Devices view SHALL display the current device's public key with a "Copy" button.
2. THE key SHALL be displayed in truncated form (first 16 chars + "…") with the full key available on hover/tap or copy.
3. IF the device identity is not yet initialised, THE extension SHALL offer to generate one.
