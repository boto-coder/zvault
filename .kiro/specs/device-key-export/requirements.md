# Requirements Document: Device Key Export & Display

## Introduction

ZVault generates a secp256k1 keypair per device for Nostr-based sync. Currently there is no way to view the device's own public key (except in the CLI's one-time `device init` output) and no way to export the keypair in standard Nostr formats (npub/nsec bech32). This feature adds key display and export across all platforms.

## Glossary

- **npub**: Nostr public key in bech32 encoding (NIP-19). Human-readable prefix `npub1`.
- **nsec**: Nostr secret key in bech32 encoding (NIP-19). Human-readable prefix `nsec1`.
- **hex pubkey**: The 64-character hex x-only public key used internally.
- **hex seckey**: The 64-character hex secret scalar (private key).
- **DeviceIdentity**: The in-memory representation of this device's keypair (UUID + pubkey).
- **SecureStorage**: OS keyring (desktop), encrypted browser.storage.local (extension), encrypted sidecar (CLI).

## Requirements

### Requirement 1: View Own Public Key

**User Story:** As a user, I want to see my device's Nostr public key at any time so I can share it or verify my identity.

#### Acceptance Criteria

1. THE desktop app SHALL display the current device's public key in the "My Device" section of the Devices page.
2. THE public key SHALL be shown in two formats: hex (64 chars, truncated with full copy) and npub (bech32, full with copy button).
3. THE browser extension SHALL display the current device's public key in its Devices view.
4. THE CLI SHALL provide a `zvault device show --vault <path>` subcommand that prints the device identity info.
5. IF no device identity exists, THE app SHALL show a prompt to generate one.
6. Viewing the public key SHALL NOT require re-authentication beyond having the vault unlocked (public keys are non-sensitive).

### Requirement 2: Copy Public Key

**User Story:** As a user, I want to easily copy my public key to share it.

#### Acceptance Criteria

1. THE desktop app SHALL provide "Copy" buttons for both hex and npub formats.
2. THE browser extension SHALL provide the same copy functionality.
3. THE CLI `device show` output SHALL be machine-parseable (key-value lines).

### Requirement 3: Export Secret Key

**User Story:** As a power user, I want to export my device's Nostr secret key (nsec) so I can use it in other Nostr clients or back it up.

#### Acceptance Criteria

1. THE desktop app SHALL provide an "Export Secret Key" action gated behind a confirmation dialog with security warning.
2. THE confirmation dialog SHALL state that the secret key grants full control of the device's Nostr identity.
3. BEFORE showing the secret key, THE app SHALL require re-authentication (password entry) even if the vault is already unlocked.
4. THE secret key SHALL be displayed in both nsec (bech32) and hex formats with copy buttons.
5. THE secret key display SHALL auto-hide after 30 seconds or when the user clicks "Done".
6. THE CLI SHALL provide `zvault device export-key --vault <path>` that prints the nsec after password confirmation.
7. THE browser extension SHALL provide the export action gated behind the same re-authentication + warning.
8. THE secret key SHALL be loaded from SecureStorage only at the moment of export, wrapped in Zeroizing, and dropped immediately after display.

### Requirement 4: NIP-19 Encoding

**User Story:** As a user, I want keys in the standard Nostr bech32 format (npub/nsec) for compatibility with the wider Nostr ecosystem.

#### Acceptance Criteria

1. THE system SHALL encode public keys as npub using bech32 with human-readable part "npub" and the 32-byte x-only key as data.
2. THE system SHALL encode secret keys as nsec using bech32 with human-readable part "nsec" and the 32-byte secret scalar as data.
3. THE encoding SHALL use bech32 (NOT bech32m) per NIP-19 specification.
4. THE system SHALL decode npub/nsec strings back to raw bytes (for future "import key" use).
5. Invalid bech32 input SHALL produce a clear error message.

### Requirement 5: Security Constraints

**User Story:** As a security-conscious user, I want secret key export to be protected.

#### Acceptance Criteria

1. THE vault MUST be unlocked for any key operation.
2. Secret key export SHALL require re-authentication (password) even with an unlocked vault.
3. The secret key SHALL never be written to logs.
4. THE CLI SHALL NOT print the nsec without password confirmation (no `--no-confirm` flag).
5. PUBLIC key display requires NO re-authentication.

### Requirement 6: CLI Output Format

**User Story:** As a CLI user, I want clean, scriptable output.

#### Acceptance Criteria

1. `zvault device show` SHALL print:
   ```
   Device ID:  <uuid>
   Label:      <label>
   Public Key: <64-char hex>
   npub:       npub1...
   ```
2. `zvault device export-key` SHALL prompt for password then print:
   ```
   nsec:       nsec1...
   Secret Hex: <64-char hex>
   ```
3. Both commands SHALL exit 1 with a message if no device identity is initialised.
4. Both commands require the vault password (the device sidecar is encrypted).
