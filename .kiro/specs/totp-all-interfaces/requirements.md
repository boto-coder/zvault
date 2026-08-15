# Requirements Document

## Introduction

This feature adds full TOTP (Time-based One-Time Password) management across all ZVault interfaces: live code generation with countdown timer, one-click copy, and add/edit of TOTP secrets. The core `generate_totp` function already exists in `zvault-core` and `zvault-wasm`; this spec surfaces it as a first-class UX element in the desktop app, browser extension, CLI, and Android app.

## Glossary

- **TOTP Code**: A 6-digit time-based one-time password generated per RFC 6238 (SHA-1, 30-second period).
- **Countdown Timer**: A visual indicator showing how many seconds remain before the current TOTP code expires and a new one is generated.
- **TOTP Secret**: A base32-encoded shared secret stored in the vault item's `totp_secret` field.
- **TotpDisplay**: A reusable UI component that shows the live TOTP code and countdown timer.

## Requirements

### Requirement 1: Desktop TOTP Code Display

**User Story:** As a desktop user viewing a login item with a TOTP secret, I want to see the current 6-digit code and a countdown showing when it expires, so that I can use it for 2FA login.

#### Acceptance Criteria

1. WHEN a login item has a non-empty `totp_secret` field, THE ItemDetail page SHALL display a TOTP section showing the current 6-digit code in large monospace font.
2. THE TOTP section SHALL display a countdown indicator showing the number of seconds remaining in the current 30-second period.
3. WHEN the 30-second period expires, THE TOTP code SHALL automatically refresh to the new code without user interaction.
4. THE TOTP section SHALL include a "Copy" button that copies the current code to the clipboard.
5. WHEN the code is copied, THE desktop app SHALL show a brief visual confirmation (e.g., button text changes to "Copied!" for 2 seconds).
6. THE TOTP section SHALL clear the clipboard after 30 seconds (matching the existing clipboard-clear behaviour).
7. WHEN a login item has no `totp_secret`, THE ItemDetail page SHALL NOT display the TOTP section.

### Requirement 2: Desktop TOTP Secret Management

**User Story:** As a desktop user, I want to add and edit TOTP secrets for my login items so that I can configure 2FA without switching to another tool.

#### Acceptance Criteria

1. WHEN editing a login item, THE edit form SHALL include a "TOTP Secret" text field.
2. WHEN adding a new login item, THE add form SHALL include an optional "TOTP Secret" field.
3. WHEN the user enters a TOTP secret, THE form SHALL validate that it is a valid base32 string before saving.
4. IF the TOTP secret is invalid (not valid base32), THEN THE form SHALL show an inline error: "Invalid TOTP secret (must be base32-encoded)".
5. WHEN a valid TOTP secret is saved, THE item detail view SHALL immediately show the live TOTP code.

### Requirement 3: CLI TOTP Code Display

**User Story:** As a CLI user, I want to quickly get the current TOTP code for an item so that I can paste it into a 2FA prompt.

#### Acceptance Criteria

1. THE `get` subcommand SHALL accept a `--totp` flag.
2. WHEN `--totp` is passed and the item has a `totp_secret`, THE CLI SHALL print the current 6-digit TOTP code and the number of seconds remaining: `TOTP: 123456 (expires in 18s)`.
3. WHEN `--totp` is passed and the item has no `totp_secret`, THE CLI SHALL print "No TOTP configured for this item" and exit with code 0.
4. WHEN `--totp` is passed, THE CLI SHALL NOT print the raw TOTP secret (regardless of whether `--show-password` is also passed).
5. THE TOTP code SHALL be calculated using the system's current UTC time with SHA-1 algorithm, 6 digits, 30-second period (matching RFC 6238 defaults).

### Requirement 4: Extension TOTP Code Display

**User Story:** As a browser extension user viewing a login item, I want to see the live TOTP code so that I can quickly copy it for 2FA without opening the desktop app.

#### Acceptance Criteria

1. WHEN the extension shows an item detail view for a login item with a `totp_secret`, THE extension SHALL display the current 6-digit TOTP code.
2. THE extension SHALL display a visual countdown indicator (numeric seconds or progress bar) showing time remaining in the current period.
3. THE TOTP code SHALL auto-refresh every 30 seconds without user interaction.
4. THE extension SHALL include a "Copy" button next to the TOTP code.
5. WHEN the code is copied, THE extension SHALL show a brief "Copied!" notification.
6. THE extension SHALL use the existing `GENERATE_TOTP` background message to obtain the code.

### Requirement 5: Extension TOTP in Item List

**User Story:** As a browser extension user, I want quick access to TOTP codes from the item list so that I can copy a code without navigating to the detail view.

#### Acceptance Criteria

1. FOR login items that have a `totp_secret`, THE item list SHALL display a small TOTP badge or icon indicating TOTP is available.
2. THE item list SHALL include a "Copy TOTP" button (clock icon or similar) alongside the existing "Copy password" button for items with TOTP.
3. WHEN the "Copy TOTP" button is pressed, THE extension SHALL generate the current TOTP code via the background worker and copy it to the clipboard.
4. WHEN the TOTP code is copied from the list, THE extension SHALL show a brief "TOTP Copied!" notification.

### Requirement 6: Tauri Backend TOTP Command

**User Story:** As a desktop frontend developer, I want a `generate_totp` Tauri command so that the React UI can request live TOTP codes from the Rust backend.

#### Acceptance Criteria

1. THE Tauri backend SHALL expose a `generate_totp` command accepting a `secret: String` parameter.
2. THE command SHALL return a JSON object: `{ code: String, remaining_seconds: u32 }`.
3. `remaining_seconds` SHALL be calculated as `30 - (current_unix_timestamp % 30)`.
4. IF the secret is not valid for TOTP generation, THEN THE command SHALL return an error string.
5. THE command SHALL use SHA-1, 6 digits, 30-second period (RFC 6238 defaults).

### Requirement 7: TOTP Secret Validation

**User Story:** As a user, I want immediate feedback if I enter an invalid TOTP secret so that I don't save broken 2FA configurations.

#### Acceptance Criteria

1. ALL interfaces (desktop, extension) SHALL validate TOTP secrets before saving.
2. Validation SHALL confirm the secret can be used to generate a TOTP code (i.e., `totp-rs::TOTP::new(...)` succeeds with the given secret).
3. IF validation fails, THE interface SHALL show an error message and prevent saving until corrected or cleared.
