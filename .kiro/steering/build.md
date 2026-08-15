# ZVault — Build & Test Steering

## Prerequisites

- **Rust** stable (1.75+) with `clippy`, `rustfmt` components
- **Node.js** 22+ and `npm`
- **wasm-pack** (`cargo install wasm-pack --locked`)
- **Tauri CLI v2** (`cargo install tauri-cli@2.1.0 --locked`)
- **wasm32-unknown-unknown** target (`rustup target add wasm32-unknown-unknown`)

---

## Workspace layout

```
zvault/
├── crates/
│   ├── zvault-core/       # Core library (Rust)
│   ├── zvault-cli/        # CLI binary (Rust, bin name: "zvault")
│   └── zvault-wasm/       # WASM bindings for browser extension
├── bindings/
│   └── uniffi/            # UniFFI bindings for Android/iOS (Kotlin/Swift)
├── apps/
│   ├── desktop/           # Tauri v2 desktop app (React + TypeScript frontend)
│   │   └── src-tauri/     # Tauri Rust backend (separate workspace)
│   ├── extension/         # Browser extension (WXT, React + TypeScript)
│   └── android/           # Android app (Kotlin + Jetpack Compose)
```

**Important:** The Tauri app (`apps/desktop/src-tauri`) has its own `[workspace]` declaration and is NOT part of the root Cargo workspace. Cargo commands at the repo root do not affect it.

---

## Core library & CLI

All commands run from the workspace root (`zvault/`).

### Build

```powershell
# Build everything (debug)
cargo build --workspace

# Build release CLI binary → target/release/zvault.exe (Windows)
cargo build --release -p zvault-cli

# Build only zvault-core
cargo build -p zvault-core
```

### Test

```powershell
# Run all workspace tests (unit + integration)
cargo test --workspace --all-features

# Run only zvault-core tests
cargo test -p zvault-core --all-features

# Run only CLI tests
cargo test -p zvault-cli

# Run a specific integration test
cargo test --test two_device_sync -p zvault-core --all-features
```

### Lint & Format

```powershell
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check    # check only
cargo fmt --all               # auto-fix
```

### Security Audit

```powershell
cargo install cargo-audit --locked
cargo audit
```

### Code Coverage

```powershell
cargo install cargo-llvm-cov --locked
cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info
```

---

## Browser extension (WXT)

Working directory: `apps/extension/`

### Prerequisites

```powershell
# Install frontend deps (from apps/extension/)
npm ci
```

### Build WASM (required before extension build)

From workspace root:

```powershell
wasm-pack build crates/zvault-wasm --target web --out-dir ../../apps/extension/public/wasm --no-typescript
# Clean generated files not needed:
Remove-Item -ErrorAction SilentlyContinue "apps/extension/public/wasm/.gitignore", "apps/extension/public/wasm/package.json"
```

### Build extension

From `apps/extension/`:

```powershell
# Chrome MV3 (default)
npx wxt build

# Firefox
npx wxt build --browser firefox

# Package as .zip for store submission
npx wxt zip               # Chrome
npx wxt zip --browser firefox  # Firefox
```

### Output locations

- Chrome: `apps/extension/.output/chrome-mv3/`
- Firefox: `apps/extension/.output/firefox-mv2/`

### Dev mode

```powershell
npx wxt                        # Chrome dev
npx wxt --browser firefox      # Firefox dev
```

### TypeScript check

```powershell
npx tsc --noEmit
```

**Note:** The `prebuild` / `prebuild:firefox` scripts in `package.json` automatically run `wasm-pack` before `wxt build`. On Windows you may need to run the wasm-pack step manually since the scripts use `rm -f` (Unix shell).

---

## Desktop app (Tauri v2)

Working directory: `apps/desktop/`

### Prerequisites

```powershell
# Install frontend deps
npm ci
```

### Build (release)

From `apps/desktop/src-tauri/`:

```powershell
cargo tauri build
```

This will:
1. Run `npm run build` (TypeScript + Vite → `apps/desktop/dist/`)
2. Compile the Tauri Rust backend in release mode
3. Produce platform installers in `apps/desktop/src-tauri/target/release/bundle/`

Windows outputs:
- `.msi` installer: `target/release/bundle/msi/`
- `.exe` NSIS installer: `target/release/bundle/nsis/`

### Dev mode

```powershell
# From apps/desktop/src-tauri/
cargo tauri dev
```

This starts Vite dev server + the Tauri window with hot reload.

### Build frontend only (no Rust)

```powershell
# From apps/desktop/
npm run build    # produces dist/
```

---

## Android (UniFFI)

### Build native library

```powershell
# Requires Android NDK and appropriate Rust targets:
# rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android

cargo build -p zvault-uniffi --target aarch64-linux-android --release
cargo build -p zvault-uniffi --target armv7-linux-androideabi --release
cargo build -p zvault-uniffi --target x86_64-linux-android --release
```

### Build Android app

From `apps/android/`:

```bash
./gradlew assembleDebug
```

Requires JDK 17 and the native `.so` files placed in `app/src/main/jniLibs/{arm64-v8a,armeabi-v7a,x86_64}/`.

---

## CI Quick Reference

CI runs on GitHub Actions (`.github/workflows/ci.yml`). Key jobs:

| Job | What it does |
|-----|------|
| `test` | `cargo fmt --check` → `cargo clippy` → `cargo test` (Linux, Windows, macOS) |
| `wasm` | `cargo build -p zvault-wasm --target wasm32-unknown-unknown` + `wasm-pack build` |
| `extension` | `npm ci` → `npx wxt build` (Chrome MV3) |
| `desktop` | `npm ci` → `cargo tauri build` (Linux, Windows, macOS) |
| `android` | UniFFI `.so` build → Gradle `assembleDebug` |
| `audit` | `cargo audit` |
| `coverage` | `cargo llvm-cov` → LCOV report |

---

## Common gotchas

1. **Tauri is a separate workspace** — `cargo build --workspace` at the repo root does NOT build the desktop app. You must `cd apps/desktop/src-tauri` and use `cargo tauri build`.

2. **WASM prebuild on Windows** — The `prebuild` npm scripts use `rm -f` which is a Unix command. On Windows/PowerShell, run the `wasm-pack` command manually then build the extension separately.

3. **rand_core version conflict** — The WASM crate and extension must use `getrandom` with the `js` feature. Both `getrandom 0.2` (for aes-gcm's rand_core 0.6) and `getrandom 0.3` (for rand 0.9) need their JS features enabled in `crates/zvault-wasm/Cargo.toml`.

4. **Desktop frontend must be built first** — `cargo tauri build` expects `apps/desktop/dist/` to exist. The `beforeBuildCommand` in `tauri.conf.json` handles this automatically, but if you build the Rust side separately you need to run `npm run build` in `apps/desktop/` first.

5. **Extension WASM must be pre-built** — The extension expects `apps/extension/public/wasm/` to contain the compiled WASM package. Always run `wasm-pack build` before `wxt build`.
