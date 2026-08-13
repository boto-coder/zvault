## Summary

<!-- What does this PR do? Why? One short paragraph. -->

## Milestone

<!-- Which milestone does this contribute to? e.g. M0, M1, M2 -->

## Changes

- 
- 

## Testing

<!-- What was tested? How? Include test names or commands run. -->

```
cargo test --workspace --all-features
```

## Security checklist

- [ ] No `unsafe` code added (enforced by `#![forbid(unsafe_code)]` in `zvault-core`)
- [ ] No secrets, credentials, or private keys committed
- [ ] New dependencies audited with `cargo audit`
- [ ] All sensitive data (passwords, keys) zeroed via `zeroize` or `ZeroizeOnDrop`
- [ ] Relevant tests added or updated

## Definition of done

- [ ] `cargo test --workspace --all-features` passes
- [ ] No `cargo clippy -- -D warnings` warnings
- [ ] `cargo audit` clean
- [ ] `cargo fmt --all -- --check` passes
- [ ] DESIGN.md updated if this PR changes the design
