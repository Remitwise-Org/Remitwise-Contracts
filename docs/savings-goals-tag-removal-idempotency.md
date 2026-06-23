# Savings Goals: `remove_tags_from_goal` Idempotency + Tag Index Cleanup

This document records the expected semantics for tag removal in `SavingsGoalContract`.

## What is guaranteed

For a goal identified by `goal_id` and owned by `caller`, `remove_tags_from_goal(caller, goal_id, tags)` has the following guarantees:

1. **Owner-only authorization**
   - Calls must be authorized by the goal owner.
   - Non-owners must fail authentication.

2. **Canonicalization matches add path**
   - Input `tags` are validated and canonicalized using the shared helper
     `remitwise_common::canonicalize_tags`.
   - Mixed-case / otherwise non-canonical inputs remove the correct
     canonical tag entries.

3. **Absent tags are a no-op**
   - Removing a tag that is not present on the goal must **not** error.
   - The goal’s tag set and any tag index entries for other tags remain unchanged.

4. **Idempotent removal (remove same tag twice)**
   - If the first call removes a tag, a second call removing the same
     (canonical) tag must be a no-op.
   - The second call must not reintroduce any removed tag into:
     - `goal.tags`
     - the `(owner, tag) -> Vec<goal_id>` tag index.

5. **Last-tag removal produces a valid empty tag set**
   - Removing the final remaining tag leaves the goal with an empty
     `goal.tags` set.

6. **No dangling tag index entries**
   - After a successful removal, the goal must not appear under the removed tag in
     `get_goals_by_tag`.
   - If the last goal ID is removed from an index entry, the underlying
     storage entry for that `(owner, tag)` key is deleted.

## Why this matters

Tag indexes are used by off-chain clients to implement efficient goal
search and filtering. Any stale or dangling index entries can cause clients to
observe “ghost” goals under removed tags.

## Tests

The following tests cover the guarantees above and must remain passing:

- `test_remove_tags_absent_tag_is_noop_and_does_not_touch_index`
- `test_remove_tags_same_tag_twice_is_idempotent_and_index_clean`
- `test_remove_last_tag_leaves_empty_tags_and_cleans_index`
- Owner-only auth failure tests for `remove_tags_from_goal`

