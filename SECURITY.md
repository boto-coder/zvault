# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.0.x   | ✅ Active |
| < 1.0   | ❌ Pre-release — not supported |

## Reporting a Vulnerability

**Do NOT open a public issue for security vulnerabilities.**

Instead, please use one of these channels:

1. **GitHub Security Advisory (preferred):**
   [Create a new advisory](https://github.com/boto-coder/zvault/security/advisories/new)

2. **Email:**
   Send details to the repository maintainers via the email listed in their GitHub profiles.

### What to include

- A description of the vulnerability and its potential impact
- Steps to reproduce or a proof-of-concept
- Affected versions (if known)
- Any suggested fix (optional but appreciated)

### Response timeline

- **Acknowledgement:** within 48 hours
- **Initial triage:** within 5 business days
- **Fix timeline:** depends on severity
  - Critical/High: patch within 7 days
  - Medium: patch within 30 days
  - Low: addressed in next scheduled release

### Severity classification

| Severity | Description |
|----------|-------------|
| Critical | Remote code execution, key material disclosure, authentication bypass |
| High | Data loss, privilege escalation, vault corruption |
| Medium | Plaintext material not zeroed, incorrect crypto parameter handling |
| Low | Best-practice violation, information disclosure with limited impact |

## Security Design

ZVault's cryptographic design is documented in [DESIGN.md](./DESIGN.md). Key decisions:

- **Vault encryption:** Argon2id (RFC 9106) + AES-256-GCM with hardware acceleration
- **Sync encryption:** NIP-44 v2 (XChaCha20-Poly1305 over ECDH/secp256k1)
- **Key zeroing:** All sensitive types implement `Zeroize` / `ZeroizeOnDrop`
- **Header authentication:** KDF params included in AES-GCM AAD to prevent downgrade attacks
- **Atomic writes:** vault files written to `.tmp` then renamed to prevent corruption

## Security Testing

- **Fuzz testing:** `cargo-fuzz` targets for decrypt, vault parser, NIP-44 decrypt, and import parsers
- **Property-based testing:** `proptest` for encrypt/decrypt roundtrips and data model invariants
- **CI:** `cargo audit` runs on every PR; `cargo clippy` with `-D warnings`
- **Code policy:** `#![forbid(unsafe_code)]` enforced in `zvault-core`

## Threat Model

See [DESIGN.md §Threat Model](./DESIGN.md) for the complete threat model covering:

- Compromised relay operators
- Device theft (locked/unlocked)
- Brute-force attacks on vault files
- Side-channel attacks
- Supply-chain attacks on dependencies
