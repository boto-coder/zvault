# ZVault — How to Build, Test, and Run

## Prerequisites
- Rust 1.75+ stable: `rustup install stable`
- cargo-audit: `cargo install cargo-audit`

## Repository layout
```
zvault/
├── Cargo.toml              # workspace root; all shared dep versions pinned here
├── crates/
│   ├── zvault-core/        # core library (crypto, vault model, device, nostr, sync, audit)
│   └── zvault-cli/         # CLI binary; thin wrapper over zvault-core
├── apps/
│   ├── desktop/            # Tauri v2 desktop app (Phase 2, not yet implemented)
│   ├── android/            # Kotlin/Jetpack Compose (Phase 3)
│   └── extension/          # WXT browser extension (Phase 3)
├── bindings/uniffi/        # UniFFI bindings for Android/iOS (Phase 3)
└── .kiro/steering/         # project steering docs (plan, tech, process, product)
```

## Build commands
```bash
# Build everything
cargo build --workspace

# Build release
cargo build --workspace --release

# Build specific crate
cargo build -p zvault-core
```

## Test commands
```bash
# Run all tests (recommended)
cargo test --workspace

# Run all tests with all features (includes test-helpers feature)
cargo test --workspace --all-features

# Run only device module tests
cargo test --package zvault-core device::

# Run a specific test
cargo test --package zvault-core device::tests::bootstrap_empty_vault

# Note: one test is intentionally ignored (slow Argon2id):
# vault::vault_file::tests::rekey_public_api_roundtrip
```

## Lint / format
```bash
cargo fmt --all               # format all code
cargo fmt --all --check       # check without modifying (CI)
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

## Dependency audit
```bash
cargo audit
```

## Feature flags (zvault-core)
- `biometric` (default): compile biometric unlock helpers
- `test-helpers`: expose InMemoryStorage and test utilities to dependent crates

## Running the CLI (stub, not fully implemented until M11)
```bash
cargo run -p zvault-cli -- --help
```

## Workspace dependency management
All dependency versions are pinned in the root `Cargo.toml` under `[workspace.dependencies]`.
Individual crates reference them with `{ workspace = true }` — never add versions directly
in a crate's `Cargo.toml`.

## CI pipeline
GitHub Actions at `.github/workflows/ci.yml` runs on every push/PR:
1. `cargo test --workspace`
2. `cargo clippy --workspace --all-targets --all-features -- -D warnings`
3. `cargo audit`
4. `cargo fmt --all --check`
