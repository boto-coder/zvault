# ZVault — Development Process Steering

## The rule: security review before every commit

**No code is committed until a security review has been run on it and all
findings of severity MEDIUM or higher are fixed.**

This applies to every milestone, every feature branch, and every hotfix.
Low and informational findings must be triaged (fixed or explicitly accepted
with a written rationale) before the commit lands on `main`.

---

## Mandatory workflow for every milestone

Follow these steps in order. Do not skip or reorder.

```
implement → security-review → fix-all-findings → verify → commit → push
```

### Step 1 — Implement

Write the code for the milestone scope. Follow the tech steering
(`tech.md`) and the milestone scope in `plan.md`.

### Step 2 — Security review

Read every file changed in the milestone. Review against the checklist below.
Produce a written findings report with severity ratings:

- **CRITICAL** — exploitable vulnerability; data loss or key compromise possible
- **MEDIUM** — functional security bug; incorrect behaviour under adversarial or
  edge-case conditions; plaintext material not zeroed; API contract violated
- **LOW** — best-practice violation; footgun with no immediate exploit
- **INFORMATIONAL** — observation worth documenting; no action required now

### Step 3 — Fix all findings

- **CRITICAL / MEDIUM:** fix before committing. No exceptions.
- **LOW:** fix before committing unless the effort exceeds the risk, in which
  case document the accepted risk explicitly in `tech.md` under the relevant
  milestone section.
- **INFORMATIONAL:** document in `tech.md` if the observation affects future
  milestones. No code change required.

### Step 4 — Verify

After fixing:

```bash
cargo build --workspace                                          # must succeed
cargo test --workspace                                           # all tests pass
cargo clippy --workspace --all-targets --all-features -- -D warnings  # zero warnings
cargo fmt --all                                                  # clean
```

If any step fails, fix it before proceeding to Step 5.

### Step 5 — Commit

Commit message format:

```
feat(<scope>): implement Mx — <short description>

<body: what was implemented>

Security review findings addressed:
- MEDIUM: <finding title> — <one-line fix summary>
- LOW: <finding title> — <one-line fix or acceptance rationale>
```

If the commit is purely a security fix on already-committed code, use:

```
fix(<scope>): address Mx security review findings

- MEDIUM: <title> — <fix summary>
- LOW: <title> — <fix summary>
```

### Step 6 — Push

```bash
git push
```

---

## Security review checklist

Run through every item for every milestone. Check off or note N/A.

### Memory safety
- [ ] All secret byte buffers (`Vec<u8>`, `String`) holding plaintext are
      wrapped in `Zeroizing<_>` or explicitly zeroed before drop
- [ ] No secret material is stored in a type that derives `Clone` or `Copy`
      without a doc warning
- [ ] Intermediate plaintext buffers from `decrypt()` are `Zeroizing<Vec<u8>>`
- [ ] JSON serialisation of sensitive structs produces a `Zeroizing<Vec<u8>>`,
      not a bare `Vec<u8>`
- [ ] `Drop` impls on sensitive structs zero all sensitive fields

### Cryptographic correctness
- [ ] No IV / nonce is reused across encryptions under the same key
- [ ] Every write generates a fresh random IV (via `encrypt()` or
      `Aes256Gcm::generate_nonce`)
- [ ] Every new vault write generates a fresh random KDF salt
      (`KdfParams::generate()`)
- [ ] The in-memory `VaultKey` is consistent with the KDF params stored in the
      on-disk blob (i.e., `save` uses `encrypt_with_params` with the *same*
      params the key was derived from, not `encrypt` which generates new params)
- [ ] `derive_key` is always called with the params parsed *from the blob being
      decrypted*, not with hardcoded or previously cached params
- [ ] No `unwrap()` in crypto paths — use `?` or `expect("invariant: ...")`

### API contracts
- [ ] Functions that accept a `VaultKey` document which KDF params the key must
      correspond to
- [ ] Functions that produce or consume plaintext document the zeroing
      responsibility
- [ ] `encrypt_with_params` callers always use a salt that was freshly generated
      (never reused across different vaults or passwords)

### Error handling
- [ ] Wrong password / corrupt file returns `Error::InvalidVaultFile`, not panic
- [ ] All `?`-propagated errors map to the correct `Error` variant
- [ ] Error messages do not leak timing information or oracle data
      (e.g., do not distinguish "wrong key" from "tampered ciphertext")

### Serialisation
- [ ] `serde` structs with sensitive fields use `#[serde(skip_serializing_if)]`
      to suppress `None` fields and reduce plaintext surface area
- [ ] Deserialisation of untrusted data (vault files from disk, Nostr events)
      maps errors to `Error::Serialisation` — never panics

### Filesystem / I/O
- [ ] All vault file writes use `atomic_write` (write-to-tmp then rename)
- [ ] No `.tmp` file is left behind after a successful write
- [ ] Temp file path appends `.tmp` to the *full filename*, not via
      `with_extension` (which replaces the last extension)

### Data model
- [ ] `delete_item` uses `Vec::remove` (order-preserving), not `swap_remove`
- [ ] `version` is incremented on every mutation (`add`, `update`, `delete`)
- [ ] Conflict detection in M4+ uses `version` counter, not timestamp equality

---

## Accepted risks register

Document findings that were accepted rather than fixed. Each entry must include
a severity, a rationale, and a re-evaluation trigger.

| Milestone | Severity | Finding | Rationale | Re-evaluate when |
|-----------|----------|---------|-----------|-----------------|
| M2 | LOW | `VaultItem::Clone` copies sensitive fields | `Clone` is required for data model usability; callers are warned in doc comments; `Drop` zeroes fields on release | M5 — before any UI layer clones items |
| M2 | INFO | Timestamps in JSON differ across devices | Version counter governs conflict detection in M4; timestamp equality is never used as a sync shortcut | M4 — document in sync design |
