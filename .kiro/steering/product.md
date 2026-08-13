# ZVault — Product Steering

## What ZVault is

ZVault is a **local-first, end-to-end encrypted password and secrets manager** that uses the [Nostr](https://nostr.com/) protocol as a permissionless, decentralised transport layer for vault synchronisation across devices.

- No server is required; no plaintext data ever leaves a device.
- Each device holds its own complete encrypted copy of the vault.
- Sync happens over Nostr relays, which see only ciphertext.

## Primary use case

A privacy-conscious user who wants a Bitwarden-style experience without trusting any third-party server or account system.

## Supported item types

- Login (username, password, TOTP, URIs)
- Secure Note
- Card (credit/debit)
- Identity (name, address, phone, email)

## Required features (v1)

- Vault create, unlock, lock, re-key
- Full CRUD on vault items
- Multi-device sync via Nostr (NIP-44, NIP-59)
- Device admit and revoke
- Passphrase-less unlock via OS biometrics (Face ID, Touch ID, Windows Hello, Android BiometricPrompt)
- Import from Bitwarden JSON, 1Password 1PUX, LastPass CSV, KeePass KDBX, generic CSV
- Encrypted export (`.zvault-export` format) and plaintext export
- Local tamper-evident audit log with HMAC hash chain
- Auto-fill on desktop (accessibility API) and in browser (content scripts, HTTPS-only)
- TOTP generation (RFC 6238)
- Clipboard clear after configurable timeout (default 30s)
- Session lock after inactivity timeout

## Platforms (v1)

1. Desktop: macOS, Windows, Linux (Tauri v2)
2. Browser extension: Chrome, Firefox, Safari (WXT)
3. Android (Kotlin / Jetpack Compose)
4. CLI (`zvault-cli`)

## Deferred to v2

- iOS client (UniFFI + Swift — same architecture as Android)
- Organisation/shared vaults with RBAC
- Field-level CRDT (Automerge)
- Relay discovery protocol
- Emergency re-key UX improvements
