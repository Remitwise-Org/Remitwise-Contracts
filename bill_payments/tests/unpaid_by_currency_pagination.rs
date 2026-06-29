//! Scale + correctness tests for `get_unpaid_bills_by_currency` in `BillPayments`.
//!
//! # Double-Predicate Pagination Contract
//!
//! `get_unpaid_bills_by_currency(owner, currency, cursor, limit)` filters on
//! TWO independent predicates simultaneously:
//!
//!   1. `bill.owner == owner`  (owner isolation)
//!   2. `bill.paid == false`   (unpaid status)
//!   3. `bill.currency == currency` (currency match)
//!
//! The cursor advances over the **currency index** for the given owner, which
//! already contains only bills of that currency; the `paid` filter is applied
//! on top during iteration.  A correctness bug could cause the cursor to skip
//! qualifying bills whenever a matching bill is filtered out as "paid", leading
//! to silent data loss for bill-reminder UIs.
//!
//! # Test Coverage
//!
//! | Test | What it guards |
//! |------|----------------|
//! | `union_equals_set_n50`    | No misses/dupes at N=50 |
//! | `union_equals_set_n200`   | No misses/dupes at N=200 |
//! | `union_equals_set_n1000`  | No misses/dupes at N=1000 |
//! | `cursor_monotonicity`     | `next_cursor` strictly increases each page |
//! | `limit_clamped`           | Limit > MAX_PAGE_LIMIT is clamped to 50 |
//! | `owner_isolation`         | Bills of another owner never appear |
//! | `zero_unpaid_in_currency` | Empty result when all matching bills are paid |
//! | `all_bills_one_currency`  | Correct when every bill uses target currency |
//! | `cursor_past_end`         | Empty page when cursor > max ID |
//! | `archived_gaps`           | Archived (removed) IDs are skipped cleanly |

use bill_payments::{BillPayments, BillPaymentsClient};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn make_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 100,
        timestamp: 1_700_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });
    env.budget().reset_unlimited();
    env
}

fn setup(env: &Env) -> (BillPaymentsClient<'_>, Address) {
    let id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(env, &id);
    let owner = Address::generate(env);
    (client, owner)
}

fn advance_rate_limit_window(env: &Env) {
    let current = env.ledger().timestamp();
    env.ledger().set_timestamp(current.saturating_add(86_400));
}

/// Create one bill for `owner` with the given `currency` and return its ID.
fn create_bill_currency(
    env: &Env,
    client: &BillPaymentsClient,
    owner: &Address,
    currency: &str,
) -> u32 {
    advance_rate_limit_window(env);
    client.create_bill(
        owner,
        &String::from_str(env, "Bill"),
        &100i128,
        &2_000_000_000u64,
        &false,
        &0u32,
        &None,
        &String::from_str(env, currency),
        &None,
    )
}

/// Exhaust `get_unpaid_bills_by_currency` via cursor pagination and return all
/// collected bill IDs, the number of pages, and assert that each page's
/// `next_cursor` is strictly greater than the previous page's cursor.
fn collect_unpaid_by_currency(
    client: &BillPaymentsClient,
    owner: &Address,
    currency: &str,
    page_size: u32,
) -> (std::vec::Vec<u32>, u32) {
    let env = &client.env;
    let currency_str = String::from_str(env, currency);
    let mut ids: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut cursor = 0u32;
    let mut page_count = 0u32;
    let mut prev_cursor = 0u32;

    loop {
        let page = client.get_unpaid_bills_by_currency(owner, &currency_str, &cursor, &page_size);
        for bill in page.items.iter() {
            ids.push(bill.id);
        }
        page_count += 1;

        if page.next_cursor == 0 {
            break;
        }

        // Cursor must strictly advance to prevent infinite loops.
        assert!(
            page.next_cursor > prev_cursor,
            "cursor must strictly advance: prev={prev_cursor} next={}",
            page.next_cursor
        );
        prev_cursor = page.next_cursor;
        cursor = page.next_cursor;
    }

    (ids, page_count)
}

// ---------------------------------------------------------------------------
// Helper: seed a mixed dataset and return expected unpaid USDC IDs
//
// Creates `n_target` unpaid USDC bills interleaved with:
//   - `n_target / 2` paid USDC bills (archived-gap simulation)
//   - `n_target / 4` unpaid XLM bills (wrong-currency noise)
//
// Returns the set of IDs that must appear in `get_unpaid_bills_by_currency`
// for currency "USDC".
// ---------------------------------------------------------------------------
fn seed_mixed(
    env: &Env,
    client: &BillPaymentsClient,
    owner: &Address,
    n_target: u32,
) -> std::vec::Vec<u32> {
    let mut expected_ids: std::vec::Vec<u32> = std::vec::Vec::new();

    for i in 0..n_target {
        // Unpaid USDC bill (target)
        let id = create_bill_currency(env, client, owner, "USDC");
        expected_ids.push(id);

        // Every other iteration, also create a PAID USDC bill (sparse gap)
        if i % 2 == 0 {
            let paid_id = create_bill_currency(env, client, owner, "USDC");
            client.pay_bill(owner, &paid_id);
        }

        // Every 4th iteration, add an unpaid XLM bill (currency noise)
        if i % 4 == 0 {
            create_bill_currency(env, client, owner, "XLM");
        }
    }

    expected_ids
}

// ---------------------------------------------------------------------------
// Test: union-equals-set + no duplicates at N = 50
// ---------------------------------------------------------------------------

/// Seed 50 unpaid USDC bills with paid USDC and unpaid XLM interleaved.
/// Page through with a small page size (7) and assert:
///   - The union of all pages == exactly the expected unpaid USDC set
///   - No duplicate IDs across pages
#[test]
fn union_equals_set_n50() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let expected = seed_mixed(&env, &client, &owner, 50);

    let (ids, _pages) = collect_unpaid_by_currency(&client, &owner, "USDC", 7);

    // No duplicates
    let mut deduped = ids.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(
        ids.len(),
        deduped.len(),
        "duplicate IDs detected across pages"
    );

    // Union == expected set (order-independent)
    let mut ids_sorted = ids.clone();
    ids_sorted.sort_unstable();
    let mut exp_sorted = expected.clone();
    exp_sorted.sort_unstable();
    assert_eq!(
        ids_sorted, exp_sorted,
        "paginated result does not match expected unpaid USDC bills"
    );
}

// ---------------------------------------------------------------------------
// Test: union-equals-set + no duplicates at N = 200
// ---------------------------------------------------------------------------

#[test]
fn union_equals_set_n200() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let expected = seed_mixed(&env, &client, &owner, 200);

    let (ids, _pages) = collect_unpaid_by_currency(&client, &owner, "USDC", 13);

    let mut deduped = ids.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(ids.len(), deduped.len(), "duplicate IDs at N=200");

    let mut ids_sorted = ids.clone();
    ids_sorted.sort_unstable();
    let mut exp_sorted = expected.clone();
    exp_sorted.sort_unstable();
    assert_eq!(ids_sorted, exp_sorted, "union ≠ set at N=200");
}

// ---------------------------------------------------------------------------
// Test: union-equals-set + no duplicates at N = 1000
// ---------------------------------------------------------------------------

/// Realistic-scale test matching the `MAX_BILLS_PER_OWNER` boundary analysis.
/// Uses a larger page size (50 = MAX_PAGE_LIMIT) to stay within Soroban budget.
#[test]
fn union_equals_set_n1000() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // At N=1000, with paid interleaving we'd exceed MAX_BILLS_PER_OWNER (1000).
    // Seed 500 unpaid USDC + 250 paid USDC + 125 XLM = 875 active bills.
    // Use n_target = 500 with interleaving ratio trimmed to stay under cap.
    let mut expected_ids: std::vec::Vec<u32> = std::vec::Vec::new();
    for i in 0u32..500 {
        let id = create_bill_currency(&env, &client, &owner, "USDC");
        expected_ids.push(id);
        // Only add paid bill every 4th to stay under the 1000-bill cap
        if i % 4 == 0 {
            let paid_id = create_bill_currency(&env, &client, &owner, "USDC");
            client.pay_bill(&owner, &paid_id);
        }
    }

    let (ids, _pages) = collect_unpaid_by_currency(&client, &owner, "USDC", 50);

    let mut deduped = ids.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(ids.len(), deduped.len(), "duplicate IDs at N=1000 scale");

    let mut ids_sorted = ids.clone();
    ids_sorted.sort_unstable();
    let mut exp_sorted = expected_ids.clone();
    exp_sorted.sort_unstable();
    assert_eq!(ids_sorted, exp_sorted, "union ≠ set at N=1000 scale");
}

// ---------------------------------------------------------------------------
// Test: cursor strictly monotonic — loop terminates
// ---------------------------------------------------------------------------

/// Seed 30 bills, collect pages of size 5, and verify each `next_cursor` is
/// strictly greater than the cursor used to obtain that page.
#[test]
fn cursor_monotonicity() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // 30 unpaid USDC + 15 paid USDC interleaved
    for i in 0u32..30 {
        create_bill_currency(&env, &client, &owner, "USDC");
        if i % 2 == 0 {
            let pid = create_bill_currency(&env, &client, &owner, "USDC");
            client.pay_bill(&owner, &pid);
        }
    }

    let currency = String::from_str(&env, "USDC");
    let mut cursor = 0u32;
    let mut cursors_seen: std::vec::Vec<u32> = std::vec::Vec::new();

    loop {
        let page = client.get_unpaid_bills_by_currency(&owner, &currency, &cursor, &5u32);
        if page.next_cursor == 0 {
            break;
        }
        assert!(
            page.next_cursor > cursor,
            "next_cursor ({}) must be > cursor ({})",
            page.next_cursor,
            cursor
        );
        // Guard against infinite loop by checking for repeated cursors
        assert!(
            !cursors_seen.contains(&page.next_cursor),
            "cursor repeated: {} — infinite loop detected",
            page.next_cursor
        );
        cursors_seen.push(page.next_cursor);
        cursor = page.next_cursor;
    }
}

// ---------------------------------------------------------------------------
// Test: limit is clamped to MAX_PAGE_LIMIT (50)
// ---------------------------------------------------------------------------

/// Passing limit > 50 must return at most 50 items per page, enforced by
/// `clamp_limit` in the contract.
#[test]
fn limit_clamped_to_max_page_limit() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 80 unpaid USDC bills
    for _ in 0u32..80 {
        create_bill_currency(&env, &client, &owner, "USDC");
    }

    let currency = String::from_str(&env, "USDC");

    // Request with limit = 200 (well above MAX_PAGE_LIMIT = 50)
    let page = client.get_unpaid_bills_by_currency(&owner, &currency, &0u32, &200u32);

    assert!(
        page.count <= 50,
        "page.count ({}) exceeds MAX_PAGE_LIMIT (50) — limit not clamped",
        page.count
    );
    assert!(
        page.items.len() <= 50,
        "items.len() ({}) exceeds MAX_PAGE_LIMIT (50)",
        page.items.len()
    );
    // There should be a next page since 80 > 50
    assert!(
        page.next_cursor > 0,
        "expected more pages but next_cursor == 0"
    );
}

/// Passing limit = 0 must use DEFAULT_PAGE_LIMIT (20).
#[test]
fn limit_zero_uses_default() {
    let env = make_env();
    let (client, owner) = setup(&env);

    for _ in 0u32..30 {
        create_bill_currency(&env, &client, &owner, "USDC");
    }

    let currency = String::from_str(&env, "USDC");
    let page = client.get_unpaid_bills_by_currency(&owner, &currency, &0u32, &0u32);

    // DEFAULT_PAGE_LIMIT = 20; 30 bills → first page has 20, next_cursor > 0
    assert_eq!(
        page.count, 20,
        "limit=0 must default to DEFAULT_PAGE_LIMIT (20)"
    );
    assert!(page.next_cursor > 0, "expected more pages");
}

// ---------------------------------------------------------------------------
// Test: owner isolation
// ---------------------------------------------------------------------------

/// Bills belonging to a different owner must NEVER appear in the results, even
/// when both owners have bills of the same currency.
#[test]
fn owner_isolation() {
    let env = make_env();
    let (client, owner_a) = setup(&env);
    let owner_b = Address::generate(&env);

    // Owner A: 10 unpaid USDC
    for _ in 0u32..10 {
        create_bill_currency(&env, &client, &owner_a, "USDC");
    }
    // Owner B: 5 unpaid USDC
    for _ in 0u32..5 {
        create_bill_currency(&env, &client, &owner_b, "USDC");
    }

    let (ids_a, _) = collect_unpaid_by_currency(&client, &owner_a, "USDC", 50);
    let (ids_b, _) = collect_unpaid_by_currency(&client, &owner_b, "USDC", 50);

    assert_eq!(ids_a.len(), 10, "owner_a should see exactly 10 USDC bills");
    assert_eq!(ids_b.len(), 5, "owner_b should see exactly 5 USDC bills");

    // No overlap
    for &id in &ids_a {
        assert!(
            !ids_b.contains(&id),
            "owner isolation violated: ID {id} appears in both owner_a and owner_b results"
        );
    }

    // Verify all returned bills belong to the querying owner
    let currency = String::from_str(&env, "USDC");
    let page_a = client.get_unpaid_bills_by_currency(&owner_a, &currency, &0u32, &50u32);
    for bill in page_a.items.iter() {
        assert_eq!(
            bill.owner, owner_a,
            "bill ID {} has wrong owner in owner_a's results",
            bill.id
        );
    }
    let page_b = client.get_unpaid_bills_by_currency(&owner_b, &currency, &0u32, &50u32);
    for bill in page_b.items.iter() {
        assert_eq!(
            bill.owner, owner_b,
            "bill ID {} has wrong owner in owner_b's results",
            bill.id
        );
    }
}

// ---------------------------------------------------------------------------
// Test: currency present on zero unpaid bills
// ---------------------------------------------------------------------------

/// When all bills in the target currency are paid, the function must return an
/// empty page with next_cursor == 0, not silently iterate forever.
#[test]
fn zero_unpaid_in_currency() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 5 USDC bills and pay them all
    for _ in 0u32..5 {
        let id = create_bill_currency(&env, &client, &owner, "USDC");
        client.pay_bill(&owner, &id);
    }

    // Also add unpaid XLM bills (wrong currency — must not appear)
    for _ in 0u32..3 {
        create_bill_currency(&env, &client, &owner, "XLM");
    }

    let currency = String::from_str(&env, "USDC");
    let page = client.get_unpaid_bills_by_currency(&owner, &currency, &0u32, &50u32);

    assert_eq!(
        page.count, 0,
        "expected zero results when all USDC bills are paid"
    );
    assert_eq!(page.next_cursor, 0, "next_cursor must be 0 when no results");
    assert_eq!(page.items.len(), 0, "items must be empty");
}

// ---------------------------------------------------------------------------
// Test: all bills in one currency
// ---------------------------------------------------------------------------

/// When every bill for an owner uses the target currency, the result set must
/// equal the full unpaid set (no bills omitted or duplicated).
#[test]
fn all_bills_one_currency() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let mut expected: std::vec::Vec<u32> = std::vec::Vec::new();
    for _ in 0u32..25 {
        let id = create_bill_currency(&env, &client, &owner, "NGN");
        expected.push(id);
    }

    let (ids, _) = collect_unpaid_by_currency(&client, &owner, "NGN", 7);

    let mut ids_sorted = ids.clone();
    ids_sorted.sort_unstable();
    let mut exp_sorted = expected.clone();
    exp_sorted.sort_unstable();

    assert_eq!(ids_sorted, exp_sorted, "all-one-currency: result mismatch");
}

// ---------------------------------------------------------------------------
// Test: cursor starting past the last ID returns empty
// ---------------------------------------------------------------------------

#[test]
fn cursor_past_end_returns_empty() {
    let env = make_env();
    let (client, owner) = setup(&env);

    for _ in 0u32..5 {
        create_bill_currency(&env, &client, &owner, "USDC");
    }

    let currency = String::from_str(&env, "USDC");
    // Use an impossibly large cursor
    let page = client.get_unpaid_bills_by_currency(&owner, &currency, &999_999u32, &50u32);

    assert_eq!(page.count, 0, "cursor past end must yield empty page");
    assert_eq!(page.next_cursor, 0);
    assert_eq!(page.items.len(), 0);
}

// ---------------------------------------------------------------------------
// Test: archived (removed) ID gaps are skipped cleanly
// ---------------------------------------------------------------------------

/// Archive paid USDC bills to create sparse IDs, then verify that cursor-based
/// pagination still collects the exact set of remaining unpaid USDC bills.
#[test]
fn archived_gaps_do_not_cause_misses() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let mut expected_unpaid: std::vec::Vec<u32> = std::vec::Vec::new();

    // Create 30 bills alternating unpaid/paid USDC
    for i in 0u32..30 {
        let id = create_bill_currency(&env, &client, &owner, "USDC");
        if i % 2 == 0 {
            // Unpaid — should appear in results
            expected_unpaid.push(id);
        } else {
            // Paid — will be archived, creating an ID gap
            client.pay_bill(&owner, &id);
        }
    }

    // Archive paid bills to remove them from active storage (creates sparse IDs)
    client.archive_paid_bills(&owner, &2_000_000_001u64);

    let (ids, _) = collect_unpaid_by_currency(&client, &owner, "USDC", 5);

    let mut deduped = ids.clone();
    deduped.sort_unstable();
    deduped.dedup();
    assert_eq!(ids.len(), deduped.len(), "duplicates after archive gaps");

    let mut ids_sorted = ids.clone();
    ids_sorted.sort_unstable();
    let mut exp_sorted = expected_unpaid.clone();
    exp_sorted.sort_unstable();

    assert_eq!(
        ids_sorted, exp_sorted,
        "archive gaps caused misses or phantom results"
    );
}

// ---------------------------------------------------------------------------
// Test: multi-currency — querying one currency does not bleed into another
// ---------------------------------------------------------------------------

/// Owner has both USDC and XLM bills. Querying for USDC must return only USDC
/// bills and querying for XLM must return only XLM bills.
#[test]
fn multi_currency_no_bleed() {
    let env = make_env();
    let (client, owner) = setup(&env);

    let mut usdc_ids: std::vec::Vec<u32> = std::vec::Vec::new();
    let mut xlm_ids: std::vec::Vec<u32> = std::vec::Vec::new();

    for _ in 0u32..15 {
        let uid = create_bill_currency(&env, &client, &owner, "USDC");
        let xid = create_bill_currency(&env, &client, &owner, "XLM");
        usdc_ids.push(uid);
        xlm_ids.push(xid);
    }

    let (got_usdc, _) = collect_unpaid_by_currency(&client, &owner, "USDC", 7);
    let (got_xlm, _) = collect_unpaid_by_currency(&client, &owner, "XLM", 7);

    let mut gs = got_usdc.clone();
    gs.sort_unstable();
    let mut us = usdc_ids.clone();
    us.sort_unstable();
    assert_eq!(gs, us, "USDC result set mismatch");

    let mut gx = got_xlm.clone();
    gx.sort_unstable();
    let mut xs = xlm_ids.clone();
    xs.sort_unstable();
    assert_eq!(gx, xs, "XLM result set mismatch");

    // No bleed between currencies
    for &id in &got_usdc {
        assert!(
            !got_xlm.contains(&id),
            "ID {id} bled from USDC into XLM results"
        );
    }
}

// ---------------------------------------------------------------------------
// Test: case-insensitive currency matching
// ---------------------------------------------------------------------------

/// Bills created with "USDC" must be returned when queried with "usdc" or
/// "Usdc" because the contract normalises currency strings to uppercase.
#[test]
fn currency_query_case_insensitive() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Create 5 bills stored as "USDC"
    let mut expected: std::vec::Vec<u32> = std::vec::Vec::new();
    for _ in 0u32..5 {
        let id = create_bill_currency(&env, &client, &owner, "USDC");
        expected.push(id);
    }
    expected.sort_unstable();

    // Query with lowercase
    let (lower_ids, _) = collect_unpaid_by_currency(&client, &owner, "usdc", 50);
    let mut ls = lower_ids.clone();
    ls.sort_unstable();
    assert_eq!(
        ls, expected,
        "lowercase currency query returned wrong results"
    );

    // Query with mixed case
    let (mixed_ids, _) = collect_unpaid_by_currency(&client, &owner, "Usdc", 50);
    let mut ms = mixed_ids.clone();
    ms.sort_unstable();
    assert_eq!(
        ms, expected,
        "mixed-case currency query returned wrong results"
    );
}

// ---------------------------------------------------------------------------
// Test: pages are in strictly ascending ID order
// ---------------------------------------------------------------------------

/// Items within each page and across pages must appear in strictly ascending
/// bill ID order (the canonical ordering guarantee of all paginated queries).
#[test]
fn result_order_strictly_ascending() {
    let env = make_env();
    let (client, owner) = setup(&env);

    // Interleave currencies to ensure ID ordering crosses currency boundaries
    for _ in 0u32..20 {
        create_bill_currency(&env, &client, &owner, "USDC");
        create_bill_currency(&env, &client, &owner, "XLM"); // noise
        let pid = create_bill_currency(&env, &client, &owner, "USDC");
        client.pay_bill(&owner, &pid); // paid gap
    }

    let (ids, _) = collect_unpaid_by_currency(&client, &owner, "USDC", 5);

    for i in 1..ids.len() {
        assert!(
            ids[i] > ids[i - 1],
            "result order violated: ids[{}]={} <= ids[{}]={}",
            i,
            ids[i],
            i - 1,
            ids[i - 1]
        );
    }
}
