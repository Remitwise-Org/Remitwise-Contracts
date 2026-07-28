# PR Description — Issue #1296: Add verify_config_migration(v) helper

## Summary

Closes #1296.

This PR implements the defense-in-depth security fix to reject reads against outdated config schema versions by adding a `verify_config_migration(v)` helper.

## Threat Mitigated
An attacker gets to bypass the config validation and trigger contract reads/behavior based on old configuration schema layouts if config schema versions are not verified before use. Specifically, version-downgrade attacks or deserializing outdated configuration layouts could lead to incorrect state interpretations or state corruptions.

## Verification
- Added a unit test `test_verify_config_migration` exercising the new check.
- Added a negative test case that ensures reads against outdated config versions return `Err(MigrationError::OutdatedVersion)`.
- Verified cargo check and clippy pass.
- Verified compilation under `wasm32-unknown-unknown` target for contracts.
