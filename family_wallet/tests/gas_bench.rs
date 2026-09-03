//! Gas benchmarks for the family_wallet contract — multisig lifecycle + archive/cleanup paths.
//!
//! Scope (issue #1431):
//! - `propose_*` cost at signer_count = 2 / 5 / 10
//! - `sign_transaction` cost (incremental step + the step that triggers execution)
//! - `archive_old_transactions` cost under varied executed-transaction volumes
//!   (10, 50, 200, plus 500 hitting the `MAX_ARCHIVE_ENTRIES` cap)
//! - `cleanup_expired_pending` cost at varied pending volumes
//! - `cancel_transaction` cost for a single pending proposal
//! - `get_pending_transactions_page` cost at first-page variant of volumes
//!
//! Pattern follows the existing per-contract `tests/gas_bench.rs` files in this
//! workspace (e.g. `bill_payments/tests/gas_bench.rs`, `savings_goals/tests/gas_bench.rs`):
//! - snapshot budget with `reset_unlimited()` + `reset_tracker()`
//! - emit a JSON line per scenario on stdout for downstream parsing
//! - assert each benchmark exercises the intended code path
//!
//! Run:
//! ```
//! RUST_TEST_THREADS=1 cargo test -p family_wallet --test gas_bench -- --nocapture
//! ```

#![allow(clippy::needless_range_loop)]

// Soroban SDK v21 wrapper note: `client.method()` for a contract method that
// returns `Result<T, Error>` will unwrap at the Rust boundary and return the
// inner `T` (panicking on Err). We use the direct wrappers here and gate the
// happy-path assertions on plain `assert!(...)` rather than `assert_eq!(..., Ok(_))`.
// The `try_*` variants stay available if any regression test needs the Result shape.

use family_wallet::{FamilyWallet, FamilyWalletClient, TransactionType};
use soroban_sdk::testutils::{Address as AddressTrait, EnvTestConfig, Ledger, LedgerInfo};
use soroban_sdk::{Address, Env, Vec};

const START_TS: u64 = 1_700_000_000;
const FAR_FUTURE_TS: u64 = 2_000_000_000;
const DEFAULT_PROPOSAL_EXPIRY_SECONDS: u64 = 86_400;
const SMALL_SPENDING_LIMIT: i128 = 1_000_000; // 1M stroops (~0.1 XLM) — keeps splits well below

// `verbatim-for-multi-call-with-non-finite-N` number of pending transactions used for
// the largest benchmarks. We use volume steps that map to real operator scenarios:
// 10 (just-over-threshold), 50 (typical busy week), 200 (busy month),
// 500 (= MAX_ARCHIVE_ENTRIES, hits the eviction policy).
const VOLUME_SMALL: u32 = 10;
const VOLUME_MEDIUM: u32 = 50;
const VOLUME_LARGE: u32 = 200;
const VOLUME_AT_CAPACITY: u32 = 500;

// Recurring benchmark setting for the multisig lifecycle suite.
const SIGNER_COUNTS: &[u32] = &[2, 5, 10];

// =====================================================================
// Environment + measurement helpers
// =====================================================================

fn bench_env() -> Env {
    let env = Env::new_with_config(EnvTestConfig {
        capture_snapshot_at_drop: false,
    });
    env.mock_all_auths();
    let proto = env.ledger().protocol_version();
    env.ledger().set(LedgerInfo {
        protocol_version: proto,
        sequence_number: 1,
        timestamp: START_TS,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });
    let mut budget = env.budget();
    budget.reset_unlimited();
    env
}

fn set_time(env: &Env, timestamp: u64) {
    env.ledger().set(LedgerInfo {
        protocol_version: env.ledger().protocol_version(),
        sequence_number: env.ledger().sequence() + 1,
        timestamp,
        network_id: [0; 32],
        base_reserve: 10,
        min_temp_entry_ttl: 1,
        min_persistent_entry_ttl: 1,
        max_entry_ttl: 100_000,
    });
}

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

fn emit_bench(method: &str, scenario: &str, cpu: u64, mem: u64) {
    // CI-friendly line with a stable prefix for downstream parsing.
    println!(
        r#"{{"contract":"family_wallet","method":"{}","scenario":"{}","cpu":{},"mem":{}}}"#,
        method, scenario, cpu, mem
    );
}

// =====================================================================
// Wallet setup helpers
// =====================================================================

/// `populated_setup` returns a tuple of (client, owner, signers) with the wallet
/// initialised and the multisig config set up for SplitConfigChange with
/// `threshold = signer_count`.
///
/// Signers are family `Member`-role addresses generated for the test. The owner
/// is intentionally NOT included in the signer set: this mirrors real-world
/// multisig governance where the owner *proposes* but quorum comes from the
/// designated signers. This pattern is what makes a propose call land in the
/// pending map (valid_signatures for the proposer = 0 < threshold) which is
/// required to benchmark the full sign→execute lifecycle.
fn populated_setup(env: &Env, signer_count: u32) -> (FamilyWalletClient, Address, Vec<Address>) {
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(env, &contract_id);

    let owner = <Address as AddressTrait>::generate(env);
    let mut initial_members: Vec<Address> = Vec::new(env);
    let mut signers: Vec<Address> = Vec::new(env);
    for _ in 0..signer_count {
        let member = <Address as AddressTrait>::generate(env);
        initial_members.push_back(member.clone());
        signers.push_back(member);
    }

    client.init(&owner, &initial_members);

    // Configure the SplitConfigChange Path as the test workhorse: it requires no
    // external token (the executor returns 0 == "success" for SplitConfigChange
    // without any side-effects), letting us reach EXEC_TXS deterministically.
    let configured = client.configure_multisig(
        &owner,
        &TransactionType::SplitConfigChange,
        &signer_count,
        &signers,
        &SMALL_SPENDING_LIMIT,
    );
    assert!(
        configured,
        "configure_multisig with valid inputs must succeed",
    );

    (client, owner, signers)
}

/// Propose a split-config-change transaction as `owner` with the canonical
/// 25/25/25/25 split. Each invocation increments `NEXT_TX`, so successive calls
/// produce successive tx_ids.
///
/// Note: the Soroban SDK wrapper for `Result<u64, Error>` returns `u64`
/// directly and panics on `Err`; callers see only the inner value.
fn propose_split(client: &FamilyWalletClient, owner: &Address) -> u64 {
    client.propose_split_config_change(owner, &25u32, &25u32, &25u32, &25u32)
}

/// Drive a pending tx to execution by signing with each signer in order.
/// `owner` is NOT a signer, so the proposal entered PEND_TXS at propose time
/// with 0 valid quorum signatures; we add `signer_count` further valid sigs
/// to reach threshold.
fn execute_pending(client: &FamilyWalletClient, signers: &Vec<Address>, tx_id: u64) {
    for i in 0..signers.len() {
        let signer = signers.get(i).unwrap();
        client.sign_transaction(&signer, &tx_id);
    }
    // After the final sign, the tx must have been moved from PEND_TXS -> EXEC_TXS.
    assert!(
        client.get_pending_transaction(&tx_id).is_none(),
        "tx must be removed from pending after reaching threshold",
    );
}

// =====================================================================
// Existing benchmark — preserved verbatim for backwards compatibility.
// =====================================================================

#[test]
fn bench_configure_multisig_worst_case() {
    let env = bench_env();
    let contract_id = env.register_contract(None, FamilyWallet);
    let client = FamilyWalletClient::new(&env, &contract_id);

    let owner = <Address as AddressTrait>::generate(&env);
    let mut initial_members = Vec::new(&env);
    let mut signers = Vec::new(&env);

    for _ in 0..8 {
        let member = <Address as AddressTrait>::generate(&env);
        initial_members.push_back(member.clone());
        signers.push_back(member);
    }

    client.init(&owner, &initial_members);

    // Include owner as an authorized signer too.
    signers.push_back(owner.clone());
    let threshold = signers.len();

    let (cpu, mem, configured) = measure(&env, || {
        client.configure_multisig(
            &owner,
            &TransactionType::LargeWithdrawal,
            &threshold,
            &signers,
            &5_000i128,
        )
    });
    assert!(configured);

    emit_bench("configure_multisig", "9_signers_threshold_all", cpu, mem);
}

// =====================================================================
// A. Propose benchmarks — proposer is `owner` (not in signer set) so the
//    proposal enters PEND_TXS and we exercise the full storage-write path.
// =====================================================================

fn bench_propose_split_config_signer_count(signer_count: u32) {
    let env = bench_env();
    let (client, owner, _signers) = populated_setup(&env, signer_count);

    let (cpu, mem, tx_id) = measure(&env, || propose_split(&client, &owner));
    assert!(
        tx_id > 0,
        "propose_split_config must return a positive tx_id when threshold > 0",
    );
    assert!(
        client.get_pending_transaction(&tx_id).is_some(),
        "proposal must be pending when proposer is not a signer and threshold > 1",
    );

    emit_bench(
        "propose_split_config",
        &format!("signer_count_{}", signer_count),
        cpu,
        mem,
    );
}

#[test]
fn bench_propose_split_config_signer_count_2() {
    bench_propose_split_config_signer_count(SIGNER_COUNTS[0]);
}

#[test]
fn bench_propose_split_config_signer_count_5() {
    bench_propose_split_config_signer_count(SIGNER_COUNTS[1]);
}

#[test]
fn bench_propose_split_config_signer_count_10() {
    bench_propose_split_config_signer_count(SIGNER_COUNTS[2]);
}

// =====================================================================
// B. Sign benchmarks (incremental step — does NOT reach quorum)
// =====================================================================
//
// The first sign call after propose adds one valid signature but does not
// reach threshold, so the proposal stays pending. This isolates the per-step
// cost (storage read/write + duplicate + expiry + role + signer-set check).

fn bench_sign_incremental_signer_count(signer_count: u32) {
    let env = bench_env();
    let (client, owner, signers) = populated_setup(&env, signer_count);
    let tx_id = propose_split(&client, &owner);

    // Pending queue now contains [owner] with 0 valid sigs.
    let signer = signers.get(0).unwrap();
    let (cpu, mem, ok) = measure(&env, || client.sign_transaction(&signer, &tx_id));
    assert!(ok);
    // Verify the tx is still pending after a single non-quorum sign.
    assert!(
        client.get_pending_transaction(&tx_id).is_some(),
        "tx must remain pending after a single non-quorum sign",
    );

    emit_bench(
        "sign_transaction_incremental",
        &format!("signer_count_{}", signer_count),
        cpu,
        mem,
    );
}

#[test]
fn bench_sign_transaction_incremental_signer_count_2() {
    bench_sign_incremental_signer_count(SIGNER_COUNTS[0]);
}

#[test]
fn bench_sign_transaction_incremental_signer_count_5() {
    bench_sign_incremental_signer_count(SIGNER_COUNTS[1]);
}

#[test]
fn bench_sign_transaction_incremental_signer_count_10() {
    bench_sign_incremental_signer_count(SIGNER_COUNTS[2]);
}

// =====================================================================
// C. Sign benchmarks (threshold-reached step — triggers execute path)
// =====================================================================
//
// After (threshold - 1) non-quorum signs by the previous signers, the next
// sign call must execute the transaction: it invokes `execute_transaction_internal`,
// removes the entry from PEND_TXS, and writes EXEC_TXS metadata. The cost
// delta over an incremental step is the largest part of the lifecycle cost.

fn bench_sign_triggers_execute_signer_count(signer_count: u32) {
    let env = bench_env();
    let (client, owner, signers) = populated_setup(&env, signer_count);
    let tx_id = propose_split(&client, &owner);

    // Sign with signers [0 .. signer_count-2]: each keeps the tx pending because
    // they push valid_sigs from 0 -> 1, 1 -> 2, ..., (signer_count-2).
    // These pre-sign calls happen *outside* the measured region — we only want to
    // measure the (signer_count)-th sign which closes the quorum and triggers
    // `execute_transaction_internal`.
    for i in 0..signer_count.saturating_sub(1) {
        let s = signers.get(i).unwrap();
        client.sign_transaction(&s, &tx_id);
    }
    assert!(
        client.get_pending_transaction(&tx_id).is_some(),
        "tx must remain pending before the final sign",
    );

    // The measured call: final signer pushes valid_sigs to threshold=N, which
    // causes execute + EXEC_TXS bookkeeping.
    let final_signer = signers.get(signer_count - 1).unwrap();
    let (cpu, mem, ok) = measure(&env, || client.sign_transaction(&final_signer, &tx_id));
    assert!(ok);
    assert!(
        client.get_pending_transaction(&tx_id).is_none(),
        "tx must be removed from pending once threshold is reached",
    );

    emit_bench(
        "sign_transaction_trigger_execute",
        &format!("signer_count_{}", signer_count),
        cpu,
        mem,
    );
}

#[test]
fn bench_sign_transaction_trigger_execute_signer_count_2() {
    bench_sign_triggers_execute_signer_count(SIGNER_COUNTS[0]);
}

#[test]
fn bench_sign_transaction_trigger_execute_signer_count_5() {
    bench_sign_triggers_execute_signer_count(SIGNER_COUNTS[1]);
}

#[test]
fn bench_sign_transaction_trigger_execute_signer_count_10() {
    bench_sign_triggers_execute_signer_count(SIGNER_COUNTS[2]);
}

// =====================================================================
// D. Archive benchmarks — volume sweep over EXEC_TXS
// =====================================================================
//
// Setup populates EXEC_TXS with N fully-executed SplitConfigChange proposals.
// We use SplitConfigChange because its executor path returns success without
// touching the token ledger, so we never need a mock token contract.
// Each archive call performs a single linear scan over EXEC_TXS + bounded
// scan over ARCH_TX (capped at MAX_ARCHIVE_ENTRIES).

fn bench_archive_executed_n(n: u32) {
    let env = bench_env();
    // Multisig setup with the smallest threshold keeps the per-tx setup cheap;
    // we only care about the executed-set size for the archive benchmark.
    let (client, owner, signers) = populated_setup(&env, 2);

    for _ in 0..n {
        let tx_id = propose_split(&client, &owner);
        execute_pending(&client, &signers, tx_id);
    }

    // All entries have executed_at = START_TS; FAR_FUTURE_TS is the cutoff so
    // the strict less-than comparison archives every entry.
    env.ledger().set_timestamp(FAR_FUTURE_TS);
    let (cpu, mem, archived_count) = measure(&env, || {
        client.archive_old_transactions(&owner, &FAR_FUTURE_TS)
    });
    assert_eq!(
        archived_count, n,
        "archive must move every executed tx whose executed_at < cutoff",
    );

    emit_bench(
        "archive_old_transactions",
        &format!("executed_n{}", n),
        cpu,
        mem,
    );
}

#[test]
fn bench_archive_executed_split_config_n10() {
    bench_archive_executed_n(VOLUME_SMALL);
}

#[test]
fn bench_archive_executed_split_config_n50() {
    bench_archive_executed_n(VOLUME_MEDIUM);
}

#[test]
fn bench_archive_executed_split_config_n200() {
    bench_archive_executed_n(VOLUME_LARGE);
}

/// Saturates `ARCH_TX` to MAX_ARCHIVE_ENTRIES = 500. This is the upper boundary
/// for the archive size cap; a follow-up call (n > 500) would additionally
/// exercise the eviction branch in `archive_old_transactions`.
#[test]
fn bench_archive_executed_split_config_n500_at_capacity() {
    bench_archive_executed_n(VOLUME_AT_CAPACITY);
}

// =====================================================================
// E. Cleanup benchmarks — expired pending proposals
// =====================================================================
//
// Default proposal expiry is 24 hours. We populate the pending map with `n`
// proposals, then advance the ledger past the expiry window so EVERY proposal
// is eligible for cleanup. Cleanup performs a single linear scan and removes
// expired entries.

fn bench_cleanup_expired_pending_n(n: u32) {
    let env = bench_env();
    let (client, owner, _signers) = populated_setup(&env, 2);

    for _ in 0..n {
        let _ = propose_split(&client, &owner);
    }

    // Advance the ledger past the default proposal expiry (24h = 86_400).
    set_time(&env, START_TS + DEFAULT_PROPOSAL_EXPIRY_SECONDS + 1);

    let (cpu, mem, removed) = measure(&env, || client.cleanup_expired_pending(&owner));
    assert_eq!(
        removed, n,
        "cleanup must remove every proposal whose expires_at < ledger_time",
    );

    emit_bench(
        "cleanup_expired_pending",
        &format!("expired_n{}", n),
        cpu,
        mem,
    );
}

#[test]
fn bench_cleanup_expired_pending_n10() {
    bench_cleanup_expired_pending_n(VOLUME_SMALL);
}

#[test]
fn bench_cleanup_expired_pending_n50() {
    bench_cleanup_expired_pending_n(VOLUME_MEDIUM);
}

// =====================================================================
// F. Cancel benchmark — owner cancels a single pending proposal
// =====================================================================
//
// Owner is privileged: it can cancel any pending proposal even if it didn't
// propose it. We use 3 signers to verify cancellation works at non-trivial
// signer count (caller's role is unaffected — cancellation is owner-privileged).

#[test]
fn bench_cancel_pending_signer_count_3() {
    let env = bench_env();
    let (client, owner, _signers) = populated_setup(&env, 3);
    let tx_id = propose_split(&client, &owner);
    assert!(client.get_pending_transaction(&tx_id).is_some());

    let (cpu, mem, ok) = measure(&env, || client.cancel_transaction(&owner, &tx_id));
    assert!(ok);
    assert!(
        client.get_pending_transaction(&tx_id).is_none(),
        "cancel must remove the proposal from PEND_TXS",
    );

    emit_bench("cancel_transaction", "single_pending", cpu, mem);
}

// =====================================================================
// G. Pagination benchmarks — list first page of pending proposals
// =====================================================================
//
// Owner is privileged and can list all pending proposals (regular members only
// see their own). `get_pending_transactions_page` walks the NEXT_TX counter
// up to a ceiling and filters by ownership; cost is bounded by the page limit
// (`DEFAULT_PENDING_PAGE_LIMIT = 20`) once the result set fills the page.

fn bench_get_pending_page_first_n(n: u32) {
    let env = bench_env();
    let (client, owner, _signers) = populated_setup(&env, 2);

    for _ in 0..n {
        let _ = propose_split(&client, &owner);
    }

    let (cpu, mem, page) = measure(&env, || {
        client.get_pending_transactions_page(&owner, &0u64, &50u32)
    });
    assert_eq!(
        page.items.len(),
        50,
        "first page of size 50 must return the requested 50 items",
    );
    assert_eq!(
        page.count,
        page.items.len(),
        "page.count and items.len() must agree for consistent pagination",
    );
    if n > 50 {
        assert!(
            page.next_cursor > 0,
            "next_cursor must point forward for n > 50"
        );
    } else {
        assert_eq!(
            page.next_cursor, 0,
            "next_cursor must be 0 when all items fit in page"
        );
    }

    emit_bench(
        "get_pending_transactions_page",
        &format!("first_page_n{}", n),
        cpu,
        mem,
    );
}

#[test]
fn bench_get_pending_page_first_n50() {
    bench_get_pending_page_first_n(VOLUME_MEDIUM);
}

#[test]
fn bench_get_pending_page_first_n200() {
    bench_get_pending_page_first_n(VOLUME_LARGE);
}
