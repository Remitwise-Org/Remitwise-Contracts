# Fixes Incoming

This task tracks the upcoming batch of fixes for the Remitwise Contracts codebase.

## Scope
- WASM compilation fixes (`wasm32-unknown-unknown` target)
- `no_std` compatibility in `remitwise-common`
- Signature verification refactor (`ed25519_verify` → Soroban host function)
- `bill_payments` contract type / ownership fixes

## Status
- [ ] `cargo build --release --target wasm32-unknown-unknown --workspace` passes
- [ ] `cargo test -p remitwise-common` passes
- [ ] CI green on macOS runner
