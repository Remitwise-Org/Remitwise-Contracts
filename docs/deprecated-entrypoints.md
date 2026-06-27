# Deprecated Soroban entrypoints and EOL markers

This document is written for downstream integrators who call Remitwise Soroban contracts and need to understand which entrypoints are being retired and what to use instead.

## What the EOL marker means

Each deprecated entrypoint now carries an `EOL:` doc-comment marker in the contract source. The marker is intended to answer three questions quickly:

- whether the entrypoint is deprecated
- when it is planned to be removed or retired
- which replacement entrypoint or flow integrators should use instead

## Current deprecated entrypoints

### Reporting contract: `get_archived_reports`

The deprecated entrypoint is `ReportingContract::get_archived_reports(env, user)`.

Example migration flow:

1. Replace calls to `get_archived_reports(user)` with `get_archived_reports_page(user, cursor, limit)`.
2. Start with `cursor = 0` and `limit = 20` (or another approved page size).
3. Continue paging by using the returned `next_cursor` until it becomes `0`.

Example replacement flow:

```rust
// Deprecated
let items = contract.get_archived_reports(env.clone(), user.clone());

// Supported
let page = contract.get_archived_reports_page(env.clone(), user.clone(), 0u32, 20u32);
let items = page.items;
```

### Guidance for integrators

- Treat deprecated entrypoints as compatibility-only APIs.
- Prefer the paginated replacement entrypoint whenever you need to read more than the first page of archived data.
- Review contract release notes and changelog entries before upgrading to a version that removes a deprecated entrypoint.

## How to read the marker

The marker appears in the Rust doc comments above the entrypoint and follows this pattern:

- `EOL:` states that the entrypoint is deprecated.
- The note includes planned removal or end-of-life guidance.
- The note points integrators to the supported replacement entrypoint or workflow.
