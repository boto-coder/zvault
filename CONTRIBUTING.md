# Contributing to ZVault

Thank you for your interest in contributing. This document describes the process, conventions, and quality bar expected for all contributions.

---

## Prerequisites

- **Rust 1.75+** stable (`rustup install stable`)
- **cargo-audit** (`cargo install cargo-audit`) — required before opening a PR
- **git** 2.x

---

## Branching

Branch from `main`. Name your branch with a conventional prefix:

| Prefix | When to use |
|---|---|
| `feat/<topic>` | New feature or capability |
| `fix/<topic>` | Bug fix |
| `chore/<topic>` | Tooling, CI, dependencies, docs |
| `test/<topic>` | Adding or improving tests |
| `refactor/<topic>` | Code restructuring without behaviour change |

Example: `feat/m1-argon2-kdf`, `fix/vault-magic-bytes`, `chore/dependabot-setup`

---

## Commit style

We use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<scope>): <short summary>

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `chore`, `docs`, `test`, `refactor`, `perf`, `ci`

Examples:
```
feat(crypto): implement Argon2id KDF (M1)
fix(vault): validate magic bytes before decryption
chore(ci): pin cargo-audit to 0.21.2
```

---

## Pull Request process

1. Branch off `main`; keep one logical change per commit where possible
2. All CI checks must pass: `cargo fmt`, `cargo clippy -D warnings`, `cargo test`, `cargo audit`
3. Every new feature or bug fix must include relevant unit tests
4. Describe what you changed and why in the PR description (use the template)
5. Request a review from a maintainer; do not merge your own PR
6. Security-sensitive code (crypto, auth, device lifecycle) requires explicit sign-off before merge

---

## Code style

```bash
# Format before committing
cargo fmt --all

# Lint — no warnings allowed
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

- `zvault-core` has `#![forbid(unsafe_code)]` — no `unsafe` blocks, ever
- All public items must have doc comments (`#![deny(missing_docs)]`)
- Sensitive data (passwords, keys) must be wrapped in `Zeroizing<_>` or a
  type that implements `ZeroizeOnDrop`

---

## Security

**Do not open public issues for security vulnerabilities.**

Email **security@zvault.app** with:
- A description of the vulnerability
- Steps to reproduce
- Your assessment of impact

We aim to acknowledge within 48 hours and issue a fix within 14 days for critical issues.

---

## Definition of done

A milestone is complete when **all** of the following are true:

- [ ] All features described in [DESIGN.md §16](./DESIGN.md) for the milestone are implemented
- [ ] `cargo test --workspace --all-features` passes
- [ ] No `cargo clippy` warnings
- [ ] `cargo audit` reports no vulnerabilities
- [ ] Relevant integration tests pass
- [ ] A PR has been reviewed and merged to `main`
- [ ] DESIGN.md updated to reflect any design decisions made during implementation

---

## Current milestone: M0 — Foundation

M0 targets:

- [x] Initialise Cargo workspace (`zvault-core`, `zvault-cli`)
- [x] GitHub Actions CI (test × 3 OS, audit, coverage)
- [x] Dependabot (Cargo + GitHub Actions)
- [x] CONTRIBUTING.md, PR template
- [ ] Branch protection rules (set in GitHub repo settings)
