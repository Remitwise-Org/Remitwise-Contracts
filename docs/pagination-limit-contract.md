# Pagination Limit Contract

`remitwise-common::clamp_limit` is the canonical normalizer for caller-supplied
pagination limits used by paginated reads across the Remitwise contracts.

The contract is:

- `0` maps to `DEFAULT_PAGE_LIMIT` (`20`).
- `1..=MAX_PAGE_LIMIT` passes through unchanged.
- Values greater than `MAX_PAGE_LIMIT` clamp to `MAX_PAGE_LIMIT` (`50`).
- The normalized output is always in `1..=MAX_PAGE_LIMIT`.
- Normalization is idempotent: `clamp_limit(clamp_limit(x)) == clamp_limit(x)`.
- `u32::MAX` and other oversized inputs clamp directly without overflow.

Callers should import and use this helper instead of duplicating pagination limit
logic locally.

## Central Bound Enforcement

In addition to `clamp_limit`, `remitwise-common::require_page_limit_within_bounds` provides explicit central validation for pagination limits:

- `require_page_limit_within_bounds(limit: u32) -> Result<(), PageLimitError>`
- Returns `Err(PageLimitError::LimitExceedsMax)` if `limit > MAX_PAGE_LIMIT`.
- Returns `Ok(())` for valid limits within `0..=MAX_PAGE_LIMIT`.

