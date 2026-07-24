## Summary

Require verifier public keys to be explicitly registered before external attestations are accepted for signature verification.

Closes #1121

## Background

An attacker that can supply a forged or untrusted verifier key could still make the contract consume an external attestation if the verifier was never explicitly registered. That would broaden the trust surface beyond the intended allow-list and create a bypass path for untrusted signatures in the attestation flow.

## Changes

### Security hardening

- Added a registered-verifier guard in `remitwise-common` so `verify_signature` now rejects unregistered verifier keys with a typed `SignatureError::UnregisteredVerifier` error.
- Added a `register_verifier` helper to allow explicit allow-listing of trusted verifier keys before attestation verification.
- Added a regression test covering the new failure mode for unregistered verifiers.

### Verification

- `cargo test -p remitwise-common`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --target wasm32-unknown-unknown --release`

## Threat model

This closes the trust-boundary gap where any externally supplied verifier public key could be treated as acceptable if the caller never registered it first. The guard ensures only explicitly approved verifiers can authorize attestation consumption.
