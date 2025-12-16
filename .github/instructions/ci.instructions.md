---
applyTo:
  - ".github/workflows/**"
  - ".github/*.yml"
---

# CI/CD Instructions

## GitHub Actions
- Use latest action versions (checkout@v6, setup-node@v6)
- Set `timeout-minutes` on all jobs
- Use `permissions` block with minimal scope
- Cache Rust builds with `Swatinem/rust-cache@v2`

## Required Checks
- Format: `cargo +nightly fmt --all -- --check`
- Lint: `cargo clippy --workspace -- -D warnings`
- Test: `cargo nextest run --workspace`
- Security: `cargo deny check`

## Secrets
- Use OIDC/Trusted Publishing when possible
- Never log secrets, use `***` masking
