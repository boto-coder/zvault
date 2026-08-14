# ZVault UniFFI Bindings

This crate (`zvault-uniffi`) exposes `zvault-core` as a C-compatible shared library via [UniFFI](https://mozilla.github.io/uniffi-rs/).  UniFFI generates foreign-language bindings (Kotlin for Android, Swift for iOS) from the UDL interface definition.

## Files

| File | Description |
|------|-------------|
| `src/zvault.udl` | Interface Definition Language — defines the FFI contract |
| `src/lib.rs` | Rust implementation of the UDL interface |
| `build.rs` | Generates UniFFI scaffolding at compile time |
| `Cargo.toml` | Crate manifest (depends on `uniffi 0.28.3`) |

## Generated Bindings

The generated Kotlin file lives at:

```
apps/android/app/src/main/java/com/zvault/uniffi/zvault.kt
```

This file is **committed to the repository** so the Android Gradle build does not require the Rust toolchain.

## Regenerating Kotlin Bindings

After modifying `src/zvault.udl`, regenerate the Kotlin bindings:

```bash
# 1. Build the native library (needed for introspection)
cargo build -p zvault-uniffi

# 2. Install uniffi-bindgen-cli (must match the crate version)
cargo install uniffi-bindgen-cli@0.28.3

# 3. Generate Kotlin bindings
uniffi-bindgen generate \
    bindings/uniffi/src/zvault.udl \
    --language kotlin \
    --out-dir apps/android/app/src/main/java/com/zvault/uniffi/ \
    --lib-file target/debug/libzvault_uniffi.so
```

Or use the convenience script:

```bash
./scripts/generate-android-bindings.sh
```

## Building the Native Library for Android

The native `.so` files are built with `cargo-ndk` and placed in the Android project's `jniLibs` directory:

```bash
# Install cargo-ndk
cargo install cargo-ndk

# Build for Android targets
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -o apps/android/app/src/main/jniLibs build -p zvault-uniffi --release
```

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌───────────────────┐
│ Kotlin (Android)│────▶│ UniFFI Bindings   │────▶│ zvault-core (Rust)│
│ VaultRepository │     │ JNA → C ABI      │     │ crypto, vault, etc│
└─────────────────┘     └──────────────────┘     └───────────────────┘
```

The Kotlin bindings use JNA to call into the native shared library (`libzvault_uniffi.so`).  The Rust side uses a handle-based API: `create_vault` / `open_vault` return an opaque `VaultHandle` that subsequent operations use to identify the session.

## UDL Interface

The current interface exposes:

- `create_vault(password, path) → VaultHandle`
- `open_vault(password, path) → VaultHandle`
- `save_vault(handle)`
- `close_vault(handle)` — zeroes key material on the Rust side
- `list_items(handle) → String` (JSON array)
- `get_item(handle, item_id) → String` (JSON object)
- `add_item(handle, item_json)`
- `delete_item(handle, item_id)`

Items are exchanged as JSON strings to keep the FFI boundary simple.  Complex type mapping (nested structs, enums with data) is handled by JSON serialisation on both sides.

## Error Handling

All functions declare `[Throws=ZVaultError]` in the UDL.  On the Kotlin side, these surface as `ZVaultException` subclasses that the `VaultRepository` catches and maps to UI states.
