//! Stress tests for bill_payments storage limits and TTL behavior.
//!
//! Issue #178: Stress Test Storage Limits and TTL
//!
//! Coverage:
//!   - Many bills per user (200+) exercising the instance-storage Map
//!   - Many bills across multiple users, verifying per-owner isolation
//!   - Instance TTL re-bump after a ledger advancement that crosses the threshold
//!   - Archive + cleanup behavior at scale (100 paid bills)
//!   - Performance benchmarks (CPU instructions + memory bytes) for key reads
//!
//! Storage layout (bill_payments):
//!   All bills live in one Map<u32, Bill> inside instance() storage.
//!   INSTANCE_BUMP_AMOUNT   = 518,400 ledgers (~30 days)
//!   INSTANCE_LIFETIME_THRESHOLD = 17,280 ledgers (~1 day)
//!   ARCHIVE_BUMP_AMOUNT    = 2,592,000 ledgers (~180 days)
//!   MAX_PAGE_LIMIT         = 50
//!   DEFAULT_PAGE_LIMIT     = 20
//!   MAX_BATCH_SIZE         = 50

use bill_payments::{BillPayments, BillPaymentsClient};
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, String};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a test environment with unlimited budget and a stable ledger.
fn stress_env() -> Env {
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

/// Reset the budget tracker and measure CPU instructions + memory bytes for `f`.
fn measure<F, R>(env: &Env, f: F) -> (u64, u64, R)
where
    F: FnOnce() -> R,
{
    let mut budget = env.budget();
    budget.reset_unlimited();
    budget.reset_tracker();
    let result = f();
    let cpu = budget.cpu_instruction_cost();
    let mem = budget.memory_bytes_cost();
    (cpu, mem, result)
}

// ---------------------------------------------------------------------------
// Stress: many entities per user
// ---------------------------------------------------------------------------

/// Create 200 bills for a single user and verify the full dataset is accessible
/// via cursor-based pagination at MAX_PAGE_LIMIT (50).
#[test]
fn stress_200_bills_single_user() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "StressBill");
    let due_date = 2_000_000_000u64; // far future

    for _ in 0..200 {
        client.create_bill(&owner, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }

    // Verify aggregate total
    let total = client.get_total_unpaid(&owner);
    assert_eq!(
        total,
        200 * 100i128,
        "get_total_unpaid must sum all 200 bills"
    );

    // Exhaust all pages with MAX_PAGE_LIMIT (50) — should take exactly 4 pages
    let mut collected = 0u32;
    let mut cursor = 0u32;
    let mut pages = 0u32;
    loop {
        let page = client.get_unpaid_bills(&owner, &cursor, &50u32);
        assert!(
            page.count <= 50,
            "Page count {} exceeds MAX_PAGE_LIMIT 50",
            page.count
        );
        collected += page.count;
        pages += 1;
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }

    assert_eq!(collected, 200, "Pagination must return all 200 bills");
    assert_eq!(pages, 4, "200 bills / 50 per page = 4 pages");
}

/// Create 200 bills for a single user and verify the instance TTL stays valid
/// after the storage Map grows to 200 entries.
#[test]
fn stress_instance_ttl_valid_after_200_bills() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "TTLBill");
    let due_date = 2_000_000_000u64;

    for _ in 0..200 {
        client.create_bill(&owner, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must remain >= INSTANCE_BUMP_AMOUNT (518,400) after 200 creates",
        ttl
    );
}

// ---------------------------------------------------------------------------
// Stress: many users
// ---------------------------------------------------------------------------

/// Create 20 bills each for 10 different users (200 total) and verify per-owner
/// totals are isolated — one user's bills do not bleed into another's.
#[test]
fn stress_bills_across_10_users() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    const N_USERS: usize = 10;
    const BILLS_PER_USER: u32 = 20;
    const AMOUNT_PER_BILL: i128 = 75;
    let due_date = 2_000_000_000u64;
    let name = String::from_str(&env, "UserBill");

    let users: Vec<Address> = (0..N_USERS).map(|_| Address::generate(&env)).collect();

    for user in &users {
        for _ in 0..BILLS_PER_USER {
            client.create_bill(user, &name, &AMOUNT_PER_BILL, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
        }
    }

    for user in &users {
        let total = client.get_total_unpaid(user);
        assert_eq!(
            total,
            BILLS_PER_USER as i128 * AMOUNT_PER_BILL,
            "Each user's total must reflect only their own bills"
        );

        // Paginate user's bills and verify count
        let mut seen = 0u32;
        let mut cursor = 0u32;
        loop {
            let page = client.get_unpaid_bills(user, &cursor, &50u32);
            seen += page.count;
            if page.next_cursor == 0 {
                break;
            }
            cursor = page.next_cursor;
        }
        assert_eq!(
            seen, BILLS_PER_USER,
            "Each user must see exactly their own {} bills via pagination",
            BILLS_PER_USER
        );
    }
}

// ---------------------------------------------------------------------------
// Stress: TTL re-bump after ledger advancement
// ---------------------------------------------------------------------------

/// Verify the instance TTL is re-bumped to >= INSTANCE_BUMP_AMOUNT (518,400)
/// after the ledger advances far enough to drop TTL below the threshold (17,280).
///
/// Phase 1: create 50 bills at sequence 100 → live_until ≈ 518,500
/// Phase 2: advance to sequence 510,000 → TTL ≈ 8,500 (below 17,280 threshold)
/// Phase 3: create 1 more bill → extend_ttl fires → TTL re-bumped to >= 518,400
#[test]
fn stress_ttl_re_bumped_after_ledger_advancement() {
    let env = stress_env(); // sequence 100, max_entry_ttl 700,000
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "TTLStress");
    let due_date = 2_000_000_000u64;

    // Phase 1: create 50 bills — TTL is set to INSTANCE_BUMP_AMOUNT
    for _ in 0..50 {
        client.create_bill(&owner, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }

    let ttl_batch1 = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl_batch1 >= 518_400,
        "TTL ({}) must be >= 518,400 after first batch of creates",
        ttl_batch1
    );

    // Phase 2: advance ledger so TTL drops below threshold
    // live_until ≈ 518,500; at sequence 510,000 → TTL ≈ 8,500 < 17,280
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 510_000,
        timestamp: 1_705_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });

    let ttl_degraded = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl_degraded < 17_280,
        "TTL ({}) must have degraded below threshold 17,280 after ledger jump",
        ttl_degraded
    );

    // Phase 3: one more create_bill triggers extend_ttl → re-bumped
    client.create_bill(&owner, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));

    let ttl_rebumped = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl_rebumped >= 518_400,
        "Instance TTL ({}) must be re-bumped to >= 518,400 after create_bill post-advancement",
        ttl_rebumped
    );
}

/// Verify TTL is also re-bumped by pay_bill after ledger advancement.
#[test]
fn stress_ttl_re_bumped_by_pay_bill_after_ledger_advancement() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "PayTTL");
    let due_date = 2_000_000_000u64;

    // Create one bill to initialise instance storage
    let bill_id = client.create_bill(&owner, &name, &500i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));

    // Advance ledger so TTL drops below threshold
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: 510_000,
        timestamp: 1_705_000_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });

    // pay_bill must re-bump TTL
    client.pay_bill(&owner, &bill_id);

    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be re-bumped to >= 518,400 after pay_bill post-advancement",
        ttl
    );
}

// ---------------------------------------------------------------------------
// Stress: archive and cleanup at scale
// ---------------------------------------------------------------------------

/// Create 100 bills, pay them all, then archive everything before a future
/// timestamp. Verify:
///   - get_storage_stats reflects the move from active → archived
///   - All archived bills are retrievable via paginated get_archived_bills
///   - Archive TTL is extended after the operation
#[test]
fn stress_archive_100_paid_bills() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "ArchiveBill");
    let due_date = 1_700_000_000u64; // same as ledger timestamp → already due

    // Create 100 bills (IDs 1..=100)
    for _ in 0..100 {
        client.create_bill(&owner, &name, &200i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }

    // Pay all 100 bills (non-recurring, so no new bills created)
    for id in 1u32..=100 {
        client.pay_bill(&owner, &id);
    }

    // Sanity: no unpaid amount remains
    assert_eq!(
        client.get_total_unpaid(&owner),
        0,
        "All bills are paid — unpaid total must be zero"
    );

    // Archive all paid bills before far-future timestamp
    let archived = client.archive_paid_bills(&owner, &2_000_000_000u64);
    assert_eq!(archived, 100, "All 100 paid bills must be archived");

    // Verify storage stats
    let stats = client.get_storage_stats();
    assert_eq!(
        stats.active_bills, 0,
        "No active bills should remain after full archive"
    );
    assert_eq!(
        stats.archived_bills, 100,
        "Storage stats must show 100 archived bills"
    );

    // Verify paginated access to archived bills
    let mut archived_seen = 0u32;
    let mut cursor = 0u32;
    loop {
        let page = client.get_archived_bills(&owner, &cursor, &50u32);
        assert!(
            page.count <= 50,
            "Archived page count {} exceeds MAX_PAGE_LIMIT 50",
            page.count
        );
        archived_seen += page.count;
        if page.next_cursor == 0 {
            break;
        }
        cursor = page.next_cursor;
    }
    assert_eq!(
        archived_seen, 100,
        "All 100 archived bills must be retrievable via paginated get_archived_bills"
    );

    // Archive operation must have re-bumped instance TTL
    let ttl = env.as_contract(&contract_id, || env.storage().instance().get_ttl());
    assert!(
        ttl >= 518_400,
        "Instance TTL ({}) must be >= 518,400 after archive_paid_bills",
        ttl
    );
}

/// Verify that archiving from multiple users works and totals are correct.
#[test]
fn stress_archive_across_5_users() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);

    const N_USERS: usize = 5;
    const BILLS_PER_USER: u32 = 20;
    let name = String::from_str(&env, "MultiUserArchive");
    let due_date = 1_700_000_000u64;

    let users: Vec<Address> = (0..N_USERS).map(|_| Address::generate(&env)).collect();

    // Create and pay bills; collect (user, bill_id) pairs
    let mut next_id = 1u32;
    let mut user_bill_ranges: Vec<(usize, u32, u32)> = Vec::new(); // (user_idx, first_id, last_id)
    for (i, user) in users.iter().enumerate() {
        let first = next_id;
        for _ in 0..BILLS_PER_USER {
            client.create_bill(user, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
            next_id += 1;
        }
        let last = next_id - 1;
        user_bill_ranges.push((i, first, last));
    }

    // Pay all bills
    for id in 1u32..next_id {
        client.pay_bill(&users[((id - 1) / BILLS_PER_USER) as usize], &id);
    }

    // Archive using first user as caller (any authenticated address may archive)
    let archived = client.archive_paid_bills(&users[0], &2_000_000_000u64);
    assert_eq!(
        archived,
        N_USERS as u32 * BILLS_PER_USER,
        "All {} bills across {} users must be archived",
        N_USERS * BILLS_PER_USER as usize,
        N_USERS
    );

    let stats = client.get_storage_stats();
    assert_eq!(stats.active_bills, 0);
    assert_eq!(stats.archived_bills, N_USERS as u32 * BILLS_PER_USER);
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Measure CPU and memory cost for fetching the first page (50 items) of
/// unpaid bills when the instance Map holds 200 entries.
#[test]
fn bench_get_unpaid_bills_first_page_of_200() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "BenchBill");
    let due_date = 2_000_000_000u64;

    for _ in 0..200 {
        client.create_bill(&owner, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }

    let (cpu, mem, page) = measure(&env, || client.get_unpaid_bills(&owner, &0u32, &50u32));
    assert_eq!(page.count, 50, "First page must return 50 bills");

    println!(
        r#"{{"contract":"bill_payments","method":"get_unpaid_bills","scenario":"200_bills_page1_50","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Measure CPU and memory cost for fetching the last page of 200 bills
/// (cursor pointing to item 150, fetching the final 50).
#[test]
fn bench_get_unpaid_bills_last_page_of_200() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "BenchBillLast");
    let due_date = 2_000_000_000u64;

    for _ in 0..200 {
        client.create_bill(&owner, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }

    // Navigate to the last page cursor
    let page1 = client.get_unpaid_bills(&owner, &0u32, &50u32);
    let page2 = client.get_unpaid_bills(&owner, &page1.next_cursor, &50u32);
    let page3 = client.get_unpaid_bills(&owner, &page2.next_cursor, &50u32);
    let cursor4 = page3.next_cursor;

    let (cpu, mem, last_page) = measure(&env, || client.get_unpaid_bills(&owner, &cursor4, &50u32));
    assert_eq!(last_page.count, 50, "Last page must return 50 bills");
    assert_eq!(last_page.next_cursor, 0, "No more pages after last page");

    println!(
        r#"{{"contract":"bill_payments","method":"get_unpaid_bills","scenario":"200_bills_last_page","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Measure CPU and memory cost of archiving 100 paid bills.
#[test]
fn bench_archive_paid_bills_100() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "ArchBench");
    let due_date = 1_700_000_000u64;

    for _ in 0..100 {
        client.create_bill(&owner, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }
    for id in 1u32..=100 {
        client.pay_bill(&owner, &id);
    }

    let (cpu, mem, result) = measure(&env, || {
        client.archive_paid_bills(&owner, &2_000_000_000u64)
    });
    assert_eq!(result, 100);

    println!(
        r#"{{"contract":"bill_payments","method":"archive_paid_bills","scenario":"100_paid_bills","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Measure CPU and memory cost of get_total_unpaid when 200 bills are in storage.
#[test]
fn bench_get_total_unpaid_200_bills() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);

    let name = String::from_str(&env, "TotalBench");
    let due_date = 2_000_000_000u64;

    for _ in 0..200 {
        client.create_bill(&owner, &name, &100i128, &due_date, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }

    let expected = 200i128 * 100;
    let (cpu, mem, total) = measure(&env, || client.get_total_unpaid(&owner));
    assert_eq!(total, expected);

    println!(
        r#"{{"contract":"bill_payments","method":"get_total_unpaid","scenario":"200_bills","cpu":{},"mem":{}}}"#,
        cpu, mem
    );
}

/// Stress test for `batch_pay_bills` with a large mixed batch (valid + invalid).
#[test]
fn stress_batch_pay_mixed_50() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner = Address::generate(&env);
    let other = Address::generate(&env);

    let name = String::from_str(&env, "BatchStress");
    let due_date = 2_000_000_000u64;

    // Create 30 valid bills for owner
    let mut valid_ids = soroban_sdk::Vec::new(&env);
    for _ in 0..30 {
        valid_ids.push_back(client.create_bill(
            &owner,
            &name,
            &100i128,
            &due_date,
            &false,
            &0u32,
            &None,
            &String::from_str(&env, "XLM"),
        ));
    }

    // Create 10 bills for 'other' (invalid for 'owner' to pay in batch)
    let mut other_ids = soroban_sdk::Vec::new(&env);
    for _ in 0..10 {
        other_ids.push_back(client.create_bill(
            &other,
            &name,
            &100i128,
            &due_date,
            &false,
            &0u32,
            &None,
            &String::from_str(&env, "XLM"),
        ));
    }

    // Mix them up with some non-existent IDs (total 50)
    let mut batch = soroban_sdk::Vec::new(&env);
    for id in valid_ids.iter() {
        batch.push_back(id);
    } // 30
    for id in other_ids.iter() {
        batch.push_back(id);
    } // 10
    for i in 0..10 {
        batch.push_back(9990 + i);
    } // 10 non-existent

    assert_eq!(batch.len(), 50);

    // Measure and execute
    let (cpu, mem, success_count) = measure(&env, || client.batch_pay_bills(&owner, &batch));

    // Only the 30 valid IDs should succeed
    assert_eq!(success_count, 30);

    println!(
        r#"{{"contract":"bill_payments","method":"batch_pay_bills","scenario":"mixed_batch_50","cpu":{},"mem":{}}}"#,
        cpu, mem
    );

    // Verify all 30 are indeed paid
    for id in valid_ids.iter() {
        assert!(client.get_bill(&id).unwrap().paid);
    }
}

/// Issue #271: Stress test for overdue bill pagination correctness.
/// Verify stable ordering, cursor correctness, and no duplication/omission.
#[test]
fn stress_overdue_bills_pagination_correctness() {
    let env = stress_env();
    let contract_id = env.register_contract(None, BillPayments);
    let client = BillPaymentsClient::new(&env, &contract_id);
    let owner1 = Address::generate(&env);
    let owner2 = Address::generate(&env);

    let name = String::from_str(&env, "StressOverdue");
    let initial_time = 1_700_000_000u64;
    
    // We will create 100 bills. 
    // Odd IDs -> due_date = initial_time + 10_000 (will be overdue when time advances past this)
    // Even IDs -> due_date = initial_time + 50_000 (will NOT be overdue)
    
    for i in 1..=100 {
        let owner = if i % 2 == 0 { &owner1 } else { &owner2 };
        let due = if i % 2 != 0 { initial_time + 10_000 } else { initial_time + 50_000 };
        client.create_bill(owner, &name, &100i128, &due, &false, &0u32, &None, &String::from_str(&env, "XLM"));
    }

    // Advance time to make odd IDs overdue
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: env.ledger().sequence(),
        timestamp: initial_time + 20_000,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 700_000,
    });

    // We should have exactly 50 overdue bills.
    // Paginate with limit = 15.
    let mut collected = std::vec::Vec::new();
    let mut cursor = 0u32;
    loop {
        let page = client.get_overdue_bills(&cursor, &15u32);
        assert!(page.count <= 15, "Page count must not exceed limit");
        for item in page.items.iter() {
            collected.push(item.id);
        }
        if page.next_cursor == 0 {
            break;
        }
        // Ensure cursor progresses positively
        assert!(page.next_cursor > cursor, "Cursor must progress forward");
        cursor = page.next_cursor;
    }

    // Verify exactly 50 overdue bills found
    assert_eq!(collected.len(), 50, "Must find exactly 50 overdue bills");
    
    // Verify no duplicates and stable ordering
    for i in 0..collected.len() - 1 {
        assert!(collected[i] < collected[i + 1], "Overdue bills must be strictly ordered by ID without duplicates");
    }

    // Verify correctness: all collected must be odd IDs (which are the overdue ones)
    for id in collected {
        assert_eq!(id % 2, 1, "Only odd IDs should be overdue");
    }
}
