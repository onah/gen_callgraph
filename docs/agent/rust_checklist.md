# Rust Development Checklist

A concise checklist to keep Rust code quality high.

## Pre-Commit
- [ ] Format code: `cargo fmt`
- [ ] Lint clean: `cargo clippy -- -D warnings`
- [ ] Tests pass: `cargo test`
- [ ] Add tests for new features
- [ ] Validate external inputs (CLI args, file paths) at the earliest boundary
- [ ] Prefer unit tests over temporary debug output (`eprintln!`, `dbg!`) for diagnosis
- [ ] Document public APIs with `///`
- [ ] Avoid `unwrap()` / `expect()` unless justified
- [ ] Do not silently ignore errors (`let _ = ...`)
- [ ] Avoid unnecessary `clone()`
- [ ] Resolve build warnings (e.g., `dead_code`)

## Quick Command
Run before committing:
```bash
cargo fmt && cargo clippy -- -D warnings && cargo test && cargo build
```

## Periodic Checks
- Security: `cargo audit`
- Outdated deps: `cargo outdated`
- Unused deps: `cargo udeps`
- Coverage (optional): `cargo tarpaulin` or `cargo-llvm-cov`

Keep this checklist brief and update it as the project evolves.