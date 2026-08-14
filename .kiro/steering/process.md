# ZVault — Development Process Steering

## Feature planning workflow

When the user requests a new feature or enhancement, follow this process:

### Step 1 — Plan (subagent)

Spawn a **planning subagent** that:
1. Reads the feature request and existing code/docs for context
2. Asks clarifying questions if the scope is ambiguous (present questions to the user, wait for answers)
3. Produces a **plan item** (see "Plan Item Format" below)
4. Appends the plan item to `plan.md` under the `## Backlog` section

### Step 2 — Confirm

Present the plan item summary to the user and ask:
> "Plan added to plan.md. Ready to start implementation?"

Do NOT start implementation until the user confirms.

### Step 3 — Implement (subagent + worktree)

For each unfinished plan item the user approves:
1. Create a git branch: `feat/<item-slug>`
2. Create a git worktree: `../zvault-<item-slug>`
3. Spawn an **implementation subagent** working in that worktree
4. The subagent follows the mandatory implementation workflow (implement → security-review → fix → verify → commit → push)

### Step 4 — Review and merge

After the implementation subagent completes:
1. Run a **security review subagent** on the worktree
2. Fix any CRITICAL/MEDIUM findings
3. Create a PR or merge directly to main (per user preference)
4. Mark the plan item as ✅ Done in `plan.md`
5. Clean up the worktree

---

## Plan Item Format

Every item in `plan.md` must follow this structure:

```markdown
### P<number> — <Short title>

**Status:** 🔲 Planned | 🚧 In Progress | ✅ Done
**Branch:** `feat/<slug>`
**Requested:** <date>

#### Description
<What the feature does, why it's needed, user-facing behaviour>

#### Scope
- <Bullet list of specific deliverables>
- <Files to create/modify>
- <Dependencies on other items>

#### Definition of Done
- [ ] <Specific testable criterion 1>
- [ ] <Specific testable criterion 2>
- [ ] All tests pass (`cargo test --workspace --all-features`)
- [ ] Zero clippy warnings
- [ ] Security review completed (no CRITICAL/MEDIUM open)
- [ ] Committed and pushed to branch

#### Expected Outputs
- <File paths of new/modified files>
- <Artifacts produced (binaries, packages, etc.)>
- <Documentation updates>
```

**Rules:**
- Each plan item is self-contained — an implementer should be able to complete it without additional context
- The Definition of Done must be concrete and verifiable (not subjective)
- Expected Outputs list every file that will be created or modified
- Items are numbered sequentially (P1, P2, P3...) and never renumbered
- Status transitions: 🔲 → 🚧 → ✅ (never backwards)

---

## Bug fixing workflow

When the user reports a bug, follow this process:

### Step 1 — Triage (subagent)

Spawn a **triage subagent** that:
1. Reads the bug description and identifies the affected module(s)
2. Reproduces the bug (writes a failing test if possible)
3. Identifies the root cause (reads relevant source code)
4. Produces a **bug item** (see "Bug Item Format" below)
5. Appends the bug item to `plan.md` under the `## Bugs` section

### Step 2 — Confirm

Present the bug analysis to the user:
> "Bug triaged — root cause: <summary>. Fix approach: <approach>. Ready to fix?"

Do NOT start fixing until the user confirms.

### Step 3 — Fix (subagent + worktree)

For each confirmed bug:
1. Create a git branch: `fix/<bug-slug>`
2. Create a git worktree: `../zvault-fix-<bug-slug>`
3. Spawn a **fix subagent** working in that worktree that:
   - Writes a regression test that reproduces the bug (test must FAIL before fix)
   - Applies the fix
   - Verifies the regression test now passes
   - Follows the mandatory implementation workflow (fix → security-review → verify → commit → push)

### Step 4 — Review and merge

After the fix subagent completes:
1. Run a **security review subagent** on the changed files
2. Fix any CRITICAL/MEDIUM findings introduced by the fix
3. Merge to main
4. Mark the bug item as ✅ Fixed in `plan.md`
5. Clean up the worktree

---

## Bug Item Format

Every bug in `plan.md` must follow this structure:

```markdown
### B<number> — <Short title>

**Status:** 🐛 Open | 🔧 Fixing | ✅ Fixed
**Branch:** `fix/<slug>`
**Reported:** <date>
**Severity:** Critical | High | Medium | Low

#### Description
<What's broken — observed behaviour vs expected behaviour>
<Steps to reproduce if applicable>

#### Root Cause
<Module, file, line — what's wrong and why>

#### Fix Approach
<How to fix it — what code changes are needed>

#### Regression Test
<Description of the test that will prevent this from recurring>

#### Definition of Done
- [ ] Regression test written (fails before fix, passes after)
- [ ] Fix applied
- [ ] No other tests broken (`cargo test --workspace --all-features`)
- [ ] Zero clippy warnings
- [ ] Security review completed (no new CRITICAL/MEDIUM)
- [ ] Committed and pushed to branch

#### Affected Files
- <File paths that will be modified>
```

**Rules:**
- Each bug is self-contained — a fixer should be able to resolve it without additional context
- The regression test is MANDATORY — no bug fix ships without a test that would catch the bug
- Root Cause must identify the specific code location, not just symptoms
- Bugs are numbered sequentially (B1, B2, B3...) and never renumbered
- Status transitions: 🐛 → 🔧 → ✅ (never backwards)
- Severity levels:
  - **Critical** — data loss, key compromise, crash on common operation
  - **High** — security degradation, incorrect sync, data corruption
  - **Medium** — wrong behaviour under specific conditions, UX breakage
  - **Low** — cosmetic, edge case, minor inconvenience

---

## The rule: security review before every commit

**No code is committed until a security review has been run on it and all
findings of severity MEDIUM or higher are fixed.**

This applies to every feature branch and every hotfix.
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
