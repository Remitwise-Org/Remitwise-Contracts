//! Unit tests for [`paginate_dependency`].
//!
//! Covers the three DataAvailability arms (Complete, Partial, Missing) and
//! the termination guarantee (non-advancing cursor still bounded by MAX_DEP_PAGES).

#![cfg(test)]

use soroban_sdk::{Env, Vec};

use crate::{paginate_dependency, DataAvailability, PaginatedResult, MAX_DEP_PAGES};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Shorthand to create a single-element page of `i128` values.
fn page(env: &Env, val: i128, next_cursor: u32) -> (Vec<i128>, u32) {
    let mut items: Vec<i128> = Vec::new(env);
    items.push_back(val);
    (items, next_cursor)
}

/// Empty page (explicitly typed to avoid type inference issues in closures).
fn empty(env: &Env, next_cursor: u32) -> (Vec<i128>, u32) {
    (Vec::new(env), next_cursor)
}

// ---------------------------------------------------------------------------
// DataAvailability::Missing — first page is empty
// ---------------------------------------------------------------------------

#[test]
fn test_empty_first_page_returns_missing() {
    let env = Env::default();
    let result = paginate_dependency(&env, |_cursor| empty(&env, 0));
    assert_eq!(result.items.len(), 0);
    assert_eq!(result.data_availability, DataAvailability::Missing);
}

#[test]
fn test_empty_first_page_nonzero_cursor_still_missing() {
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |_cursor| {
        call_count += 1;
        if call_count == 1 {
            empty(&env, 1)
        } else {
            empty(&env, 0)
        }
    });
    assert_eq!(result.items.len(), 0);
    assert_eq!(result.data_availability, DataAvailability::Missing);
}

// ---------------------------------------------------------------------------
// DataAvailability::Complete — drained within MAX_DEP_PAGES
// ---------------------------------------------------------------------------

#[test]
fn test_single_page_complete() {
    let env = Env::default();
    let result = paginate_dependency(&env, |_cursor| page(&env, 42, 0));
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.data_availability, DataAvailability::Complete);
}

#[test]
fn test_multiple_pages_complete() {
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |_cursor| {
        call_count += 1;
        if call_count < 5 {
            page(&env, call_count as i128, call_count)
        } else {
            page(&env, call_count as i128, 0)
        }
    });
    assert_eq!(result.items.len(), 5);
    assert_eq!(result.data_availability, DataAvailability::Complete);
}

#[test]
fn test_max_pages_still_complete() {
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |cursor| {
        call_count += 1;
        if call_count < MAX_DEP_PAGES {
            page(&env, call_count as i128, cursor + 1)
        } else {
            page(&env, call_count as i128, 0)
        }
    });
    assert_eq!(result.items.len(), MAX_DEP_PAGES);
    assert_eq!(result.data_availability, DataAvailability::Complete);
}

// ---------------------------------------------------------------------------
// DataAvailability::Partial — page cap reached
// ---------------------------------------------------------------------------

#[test]
fn test_exactly_max_pages_returns_partial() {
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |cursor| {
        call_count += 1;
        page(&env, call_count as i128, cursor + 1)
    });
    assert_eq!(call_count, MAX_DEP_PAGES);
    assert_eq!(result.items.len(), MAX_DEP_PAGES);
    assert_eq!(result.data_availability, DataAvailability::Partial);
}

#[test]
fn test_exceeds_max_pages_returns_partial() {
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |_cursor| {
        call_count += 1;
        page(&env, call_count as i128, call_count)
    });
    assert_eq!(call_count, MAX_DEP_PAGES);
    assert_eq!(result.data_availability, DataAvailability::Partial);
}

// ---------------------------------------------------------------------------
// Termination guarantee — non-advancing cursor
// ---------------------------------------------------------------------------

#[test]
fn test_non_advancing_cursor_terminates_at_partial() {
    // A buggy dependency that always returns cursor=1 (never 0, never advancing).
    // The helper must still terminate after MAX_DEP_PAGES.
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |_cursor| {
        call_count += 1;
        page(&env, call_count as i128, 1)
    });
    assert_eq!(call_count, MAX_DEP_PAGES);
    assert_eq!(result.items.len(), MAX_DEP_PAGES);
    assert_eq!(result.data_availability, DataAvailability::Partial);
}

#[test]
fn test_non_advancing_cursor_with_zero_items_terminates() {
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |_cursor| {
        call_count += 1;
        empty(&env, 1)
    });
    assert_eq!(call_count, MAX_DEP_PAGES);
    assert_eq!(result.items.len(), 0);
    assert_eq!(result.data_availability, DataAvailability::Partial);
}

// ---------------------------------------------------------------------------
// Edge: first page non-empty, then pages empty until cursor=0
// ---------------------------------------------------------------------------

#[test]
fn test_first_page_has_items_rest_empty() {
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |_cursor| {
        call_count += 1;
        if call_count == 1 {
            page(&env, 99, 1)
        } else {
            empty(&env, 0)
        }
    });
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.data_availability, DataAvailability::Complete);
}

// ---------------------------------------------------------------------------
// Edge: empty pages in the middle (cursor advances past empty pages)
// ---------------------------------------------------------------------------

#[test]
fn test_skip_empty_middle_pages() {
    let env = Env::default();
    let mut call_count = 0u32;
    let result = paginate_dependency(&env, |cursor| {
        call_count += 1;
        match call_count {
            1 => page(&env, 10, cursor + 1),
            2 => empty(&env, cursor + 1),
            3 => page(&env, 20, 0),
            _ => empty(&env, 0),
        }
    });
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.data_availability, DataAvailability::Complete);
}

// ---------------------------------------------------------------------------
// PaginatedResult struct fields
// ---------------------------------------------------------------------------

#[test]
fn test_paginated_result_struct_fields() {
    let env = Env::default();
    let items: Vec<i128> = Vec::new(&env);
    let pr = PaginatedResult {
        items,
        data_availability: DataAvailability::Complete,
    };
    assert_eq!(pr.items.len(), 0);
    assert_eq!(pr.data_availability, DataAvailability::Complete);
}
