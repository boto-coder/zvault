# Design Document: TOTP All Interfaces

## Architecture Overview

TOTP generation already exists in the stack:
- `zvault-core`: `totp-rs` crate available
- `zvault-wasm`: `generate_totp(secret) -> code` exposed via `wasm_bindgen`
- Extension: `GENERATE_TOTP` background message handler exists

This feature adds:
- A Tauri `generate_totp` command (desktop backend → frontend bridge)
- UI components (desktop `TotpDisplay`, extension TOTP in detail/list)
- CLI `--totp` flag
- Validation logic for TOTP secrets
- Timer-based auto-refresh in UIs

```
┌─────────────────────────────────────────────────────────┐
│              Desktop (Tauri + React)                      │
│                                                           │
│  ItemDetail                                               │
│    └─ TotpDisplay                                        │
│         ├─ calls invoke("generate_totp") every 1s        │
│         ├─ displays 6-digit code (monospace)             │
│         ├─ countdown ring/bar (30s period)               │
│         └─ Copy button + clipboard clear                 │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│           Extension (WXT + React)                        │
│                                                           │
│  ItemDetailView                                           │
│    └─ TotpDisplay                                        │
│         ├─ sends GENERATE_TOTP msg every 1s              │
│         ├─ displays code + countdown                     │
│         └─ Copy button + toast                           │
│                                                           │
│  ItemListView                                             │
│    └─ TOTP badge + "Copy TOTP" button per item          │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│                CLI (Rust)                                 │
│                                                           │
│  zvault get <id> --totp --vault <path>                   │
│    └─ totp_rs::TOTP::new(...).generate(now)             │
│    └─ print "TOTP: 123456 (expires in 18s)"            │
└─────────────────────────────────────────────────────────┘
```

## Components

### 1. Tauri `generate_totp` Command (`apps/desktop/src-tauri/src/main.rs`)

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TotpResponse {
    code: String,
    remaining_seconds: u32,
}

#[tauri::command]
fn generate_totp(secret: String) -> Result<TotpResponse, String> {
    use totp_rs::{Algorithm, TOTP};

    let totp = TOTP::new(Algorithm::SHA1, 6, 1, 30, secret.as_bytes().to_vec())
        .map_err(|e| format!("Invalid TOTP secret: {e}"))?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let code = totp.generate(now);
    let remaining = 30 - (now % 30) as u32;

    Ok(TotpResponse { code, remaining_seconds: remaining })
}
```

### 2. Tauri `validate_totp_secret` Command

```rust
#[tauri::command]
fn validate_totp_secret(secret: String) -> Result<(), String> {
    use totp_rs::{Algorithm, TOTP};

    TOTP::new(Algorithm::SHA1, 6, 1, 30, secret.as_bytes().to_vec())
        .map_err(|e| format!("Invalid TOTP secret: {e}"))?;

    Ok(())
}
```

### 3. Desktop `TotpDisplay` Component (`apps/desktop/src/components/TotpDisplay.tsx`)

```typescript
interface TotpDisplayProps {
  secret: string;
}

function TotpDisplay({ secret }: TotpDisplayProps) {
  const [code, setCode] = useState("------");
  const [remaining, setRemaining] = useState(30);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    const refresh = async () => {
      try {
        const result = await invoke<{ code: string; remainingSeconds: number }>(
          "generate_totp", { secret }
        );
        setCode(result.code);
        setRemaining(result.remainingSeconds);
      } catch (err) {
        setCode("ERROR");
      }
    };
    refresh();
    const interval = setInterval(refresh, 1000);
    return () => clearInterval(interval);
  }, [secret]);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
    // Clear clipboard after 30s
    setTimeout(() => navigator.clipboard.writeText(""), 30000);
  };

  return (
    <div className="flex items-center gap-3 p-3 bg-gray-100 dark:bg-gray-700 rounded-lg">
      <div className="flex-1">
        <label className="block text-sm font-medium text-gray-500 dark:text-gray-400 mb-1">
          TOTP Code
        </label>
        <div className="text-2xl font-mono font-bold text-gray-900 dark:text-gray-100 tracking-wider">
          {code.slice(0, 3)} {code.slice(3)}
        </div>
      </div>
      <div className="text-sm text-gray-500 dark:text-gray-400 text-center">
        <div className="text-lg font-mono">{remaining}s</div>
        <div className="text-xs">remaining</div>
      </div>
      <button
        type="button"
        onClick={handleCopy}
        className="px-3 py-1.5 text-sm bg-zvault-600 hover:bg-zvault-700 text-white rounded-lg transition-colors"
      >
        {copied ? "Copied!" : "Copy"}
      </button>
    </div>
  );
}
```

### 4. CLI `--totp` Flag Implementation

```rust
// In cmd_get, after displaying item fields:
if totp {
    if let Some(secret) = &item.totp_secret {
        let totp_gen = totp_rs::TOTP::new(
            totp_rs::Algorithm::SHA1, 6, 1, 30,
            secret.as_bytes().to_vec()
        ).map_err(|e| anyhow::anyhow!("invalid TOTP secret: {e}"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
        let code = totp_gen.generate(now);
        let remaining = 30 - (now % 30);

        println!("  TOTP:     {} (expires in {}s)", code, remaining);
    } else {
        println!("  No TOTP configured for this item.");
    }
    // When --totp is passed, suppress showing the raw secret
}
```

### 5. Extension TOTP in Item Detail

The extension already has a `GENERATE_TOTP` handler. The detail view will poll it:

```typescript
function TotpSection({ secret }: { secret: string }) {
  const [code, setCode] = useState("------");
  const [remaining, setRemaining] = useState(30);

  useEffect(() => {
    const refresh = async () => {
      const response = await browser.runtime.sendMessage({
        type: "GENERATE_TOTP",
        payload: { secret },
      });
      if (response.code) {
        setCode(response.code);
        setRemaining(response.remainingSeconds || (30 - Math.floor(Date.now() / 1000) % 30));
      }
    };
    refresh();
    const interval = setInterval(refresh, 1000);
    return () => clearInterval(interval);
  }, [secret]);

  // ... render code + countdown + copy button
}
```

### 6. Extension TOTP Background Handler Update

Update the existing `GENERATE_TOTP` handler to also return `remainingSeconds`:

```typescript
case "GENERATE_TOTP": {
  const { secret } = message.payload as { secret: string };
  const { initWasm } = await import("../lib/wasm");
  const wasm = await initWasm();
  const code = wasm.generate_totp(secret);
  const now = Math.floor(Date.now() / 1000);
  const remainingSeconds = 30 - (now % 30);
  return { code, remainingSeconds };
}
```

## Interfaces

### Tauri Command Interface

```typescript
// generate_totp
invoke("generate_totp", { secret: "JBSWY3DPEHPK3PXP" })
// → { code: "123456", remainingSeconds: 18 }

// validate_totp_secret
invoke("validate_totp_secret", { secret: "JBSWY3DPEHPK3PXP" })
// → null (success) or throws error string
```

### Extension Message Protocol

```typescript
// Request
browser.runtime.sendMessage({ type: "GENERATE_TOTP", payload: { secret: "..." } })

// Response (updated — backward compatible addition)
{ code: "123456", remainingSeconds: 18 }
// or
{ error: "invalid TOTP secret: ..." }
```

### CLI Interface

```
$ zvault get 550e8400-... --totp --vault ~/my.zvault
  TOTP:     482916 (expires in 22s)

$ zvault get 550e8400-... --totp --vault ~/my.zvault  # no TOTP configured
  No TOTP configured for this item.
```

## Error Handling

| Error | Source | Handling |
|---|---|---|
| Invalid base32 secret | `TOTP::new()` | Show inline error, prevent save |
| Empty secret string | Validation | Skip TOTP display (no error) |
| System clock issue | `SystemTime::now()` | Fall back to 0 remaining, log warning |
| WASM TOTP generation failure | Extension background | Return `{ error: "..." }`, show in UI |
