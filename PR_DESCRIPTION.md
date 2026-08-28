# PR Description — Issue #1297: Central enforcement for MAX_PAGE_LIMIT

## Summary
Closes #1297.

This PR implements central enforcement for `MAX_PAGE_LIMIT` via a typed error (`PageLimitError::LimitExceedsMax`) and guard function (`require_page_limit_within_bounds`) in `remitwise-common`.

## Threat Mitigated
**Resource Exhaustion / Denial of Service (DoS)**: Without central bound enforcement on pagination limits, an attacker could supply oversized `limit` arguments (up to `u32::MAX`). If a contract entry point fails to clamp or check bounds, processing oversized limits could lead to excessive storage iterations, high host CPU/memory consumption, and Denial of Service (DoS) / gas exhaustion on Soroban smart contracts.

## Changes Made
- **Typed Error**: Added `PageLimitError` (`#[contracterror]`, discriminant `LimitExceedsMax = 1`) in `remitwise-common`.
- **Guard Helper**: Added `require_page_limit_within_bounds(limit: u32) -> Result<(), PageLimitError>` in `remitwise-common`.
- **Positive Tests**: Added `test_require_page_limit_within_bounds_valid` covering boundary cases `0`, `1`, `DEFAULT_PAGE_LIMIT`, and `MAX_PAGE_LIMIT`.
- **Negative Tests**: Added `test_require_page_limit_within_bounds_exceeded_negative` covering `MAX_PAGE_LIMIT + 1`, `100`, and `u32::MAX`.
- **Documentation**: Updated `docs/pagination-limit-contract.md`.

## Execution & Gas Cost Measurement
- **Hot Path Cost**: `require_page_limit_within_bounds` performs a single `u32` comparison (`limit > MAX_PAGE_LIMIT`). Cost estimate is 1 CPU comparison instruction (~0 gas overhead).

## Local Verification
- `cargo test -p remitwise-common test_require_page_limit_within_bounds`: Passed (2 tests ok).
- `cargo build -p remitwise-common --target wasm32-unknown-unknown --release`: Success.
