# Recurring Payment Idempotency QE Note — August 2026

## Scope and API surface

This change adds caller-supplied `BytesN<32>` request keys to bill and bill-schedule mutations so clients can safely retry after a timeout or lost response. The keyed APIs are:

- `create_bill_keyed`
- `pay_bill_keyed`
- `cancel_bill_keyed`
- `restore_bill_keyed`
- `create_bill_schedule_keyed`
- `modify_bill_schedule_keyed`
- `cancel_bill_schedule_keyed`
- `execute_due_bill_schedules_keyed`

The principal QE scenarios are recurring `pay_bill_keyed` execution and keyed due-schedule execution. The existing unkeyed APIs remain available for compatibility.

## Typed, actor-scoped persistent journal

Each successful keyed call writes one persistent journal entry under the typed key:

```text
RequestJournalKey { actor: Address, request_key: BytesN<32> }
```

The value is a `RequestJournalEntry` containing the complete typed `BillPaymentRequest` and its typed `BillPaymentResult`. Request variants distinguish operations and include every argument that determines semantics. Result variants preserve bill IDs, payment receipts, unit results, booleans, or schedule-ID vectors without lossy encoding.

Actor scoping means two authenticated actors may independently use the same 32-byte key. Within one actor's namespace, a key is bound to exactly one complete request. Keys are identifiers, not credentials or entropy-based authorization tokens.

## Lookup, authorization, guards, and commit order

All keyed entry points follow this order:

1. Require authorization from the actor that owns the journal namespace.
2. Build the complete typed request.
3. Look up `(actor, request_key)` in persistent storage.
4. If the stored request is exactly equal, extend its TTL and return the stored result immediately.
5. If the key exists with a different request, return `RequestKeyConflict`.
6. On a miss, run operation-specific guards and validation.
7. Execute the core state transition and emit its normal domain events.
8. Persist the request and successful result, then extend the journal TTL.
9. Return the newly committed result.

For `pay_bill_keyed`, the miss-path guards are kill switch, trusted-orchestrator/cross-contract epoch, and function pause, followed by payment execution and journal commit. Caller authentication intentionally precedes lookup. For `execute_due_bill_schedules_keyed`, caller authentication and lookup precede global-pause and kill-switch checks, then bounded schedule execution and commit. Unlike the legacy executor's empty-vector pause response, a fresh keyed execution returns `ContractPaused` and does not bind its key, allowing the same request to execute after unpause.

An exact replay returns before mutable business guards and core execution. This is necessary for a client to recover the already-committed receipt even if the epoch, pause, kill-switch, ledger time, or underlying bill state changed after the first success. It does not authorize a new state transition.

## Idempotency and conflict invariants

- An exact retry by the same actor with the same key and identical typed request returns the exact stored result.
- An exact retry does not repeat a payment, mint another recurring child, advance a schedule again, update unpaid totals again, or emit the operation's domain events again.
- Reusing a key for another operation or changing any request field returns `RequestKeyConflict`.
- A conflicting request cannot read another actor's receipt because the actor is part of the storage key and must authorize before lookup.
- The same raw key used by different actors identifies independent journal entries.
- The first successful serialized invocation determines the key binding. Later exact invocations replay it; later differing invocations conflict.

## Failed operations and events

Only successful core operations are journaled. Validation, authorization, pause, kill-switch, epoch, ownership, missing-record, and arithmetic failures do not create a successful receipt, so a corrected request may reuse a key that was never committed.

An exact lookup refreshes the existing receipt TTL. A conflicting lookup does not refresh or replace the entry and emits no domain event, so rejection is mutation-free. Fresh-operation events occur in the core transition before journal commit within the same contract invocation; a trapped invocation is atomic, so it cannot leave a committed partial transition. Exact replay bypasses the core and emits no duplicate payment or schedule event.

## Positive schedule amounts and legacy compatibility

New schedule creation and schedule modification now require `amount > 0`; zero and negative amounts return `InvalidAmount`. This aligns schedule-generated bills with the existing positive bill-amount invariant.

Historical storage may still contain schedules created before this validation existed. Due-schedule execution does not delete, mutate, or mint from a legacy schedule whose amount is non-positive. It skips that schedule, leaving it inspectable and cancellable. Operators should cancel it or modify it to a positive amount before expecting execution. No eager migration rewrites legacy schedule data.

## Concurrency, serialization, timeout, and reorder behavior

Soroban transactions are applied in deterministic ledger order; there is no shared-memory race inside one contract invocation. If duplicate submissions compete, the first successful transaction commits the journal. A later exact duplicate reads that receipt and has no second effect. A later request with the same actor/key but different parameters deterministically conflicts.

A client may retry after a timeout without knowing whether the original transaction committed. If it committed, the retry returns the frozen receipt; if it did not commit, the retry executes normally. Delayed or reordered exact duplicates remain safe while the journal entry exists. Reordered non-identical requests sharing one key are intentionally not last-write-wins: whichever valid request commits first binds the key and the other conflicts.

Schedule execution receipts freeze the original vector of executed schedule IDs. A delayed replay returns that vector even after ledger time advances and additional schedules become due. A successful empty execution also binds `[]`, including when no schedule is due or all due schedules contain invalid legacy amounts. Repairing a skipped schedule does not change that receipt; new due work requires a new request key.

## TTL, storage, and resource limitations

Journal entries use persistent storage. On creation and lookup they request the common persistent policy: extend when remaining TTL is below 15 days, targeting 60 days. Exact-retry guarantees therefore depend on the entry remaining live. After archival or expiry, the same key can be treated as absent and may execute again; clients needing longer deduplication must retry within the retention window or maintain an external record.

Every distinct actor/key consumes a persistent ledger entry containing the full typed request and result. There is no on-chain enumeration or explicit journal deletion API in this change. Callers should not generate keys for operations they do not intend to submit, and should budget for persistent-entry rent.

Schedule execution itself remains bounded by `MAX_BATCH_SIZE` and its execution cursor. One keyed receipt records only that invocation's returned schedule IDs. Large backlogs require multiple calls with distinct keys. CPU and memory vary with request shape, storage footprint, and host/protocol version; benchmark thresholds are explicit first-pass regression bounds, not fee guarantees.

## Migration and rollback

The keyed methods are additive and require no eager transformation of existing bills or schedules. Existing clients can continue using unkeyed methods. New persistent journal entries are written only after keyed successes.

Deployment should update generated bindings and clients together, then direct retry-capable callers to stable, unique keys. During rollout, do not switch a logical operation between keyed and unkeyed APIs: the unkeyed path cannot consult the journal.

Rolling back to a contract version without keyed APIs makes journal entries unreachable but does not reinterpret existing bill or schedule state. The entries remain subject to ledger TTL/rent. Re-deploying the keyed version before expiry restores their use. Clients must stop keyed traffic before rollback and must not replay the same logical operation through an unkeyed method. Legacy non-positive schedules require no rollback migration because execution already leaves them untouched.

## Security assumptions

- The actor's Soroban authorization is the security boundary; possession or prediction of a request key grants no authority.
- Clients must generate a unique key per logical operation and retain the complete request associated with it.
- Typed full-request equality prevents cross-operation and changed-argument replay under one actor/key.
- `pay_bill_keyed` still requires a configured trusted orchestrator and matching cross-contract epoch on the first execution.
- Exact replay intentionally precedes mutable guards and returns only a previously committed result; it cannot produce a new payment while paused, killed, or on a stale epoch.
- Journal retention is finite, so this mechanism is bounded retry deduplication rather than permanent global uniqueness.
- Ledger ordering and transaction atomicity are trusted. Off-chain concurrent senders must coordinate key assignment to avoid intentional conflicts.

## Test coverage

Functional coverage includes:

- keyed recurring creation exact retry, semantic conflict, actor isolation, and corrected retry after an uncommitted failure;
- keyed recurring payment timeout retry with a stable `AtomicPayReceipt`, one child bill, unchanged unpaid totals, no extra events, conflict rejection, and actor isolation;
- keyed cancel and restore exact retries with one state transition and one event;
- keyed schedule create, modify, cancel, conflict, failed-retry, and ownership behavior;
- keyed due-schedule timeout retry with frozen schedule IDs, unchanged generated bill, unchanged schedule advancement, unchanged unpaid total, and no extra events;
- keyed pause rejection without key consumption, followed by successful reuse after unpause;
- legacy non-positive schedule isolation, frozen empty replay, authorized repair, and execution under a fresh key;
- property-based repeated keyed creation proving one effect across multiple retries.

`bill_payments/tests/gas_bench.rs` adds bounded CPU and memory scenarios for the first execution and exact replay of both recurring `pay_bill_keyed` and `execute_due_bill_schedules_keyed`. The payment benchmark configures the trusted orchestrator and cross-contract epoch inside the registered contract context. Both benchmarks use `BytesN<32>` keys and assert that replay has no second business effect.

## Validation commands and current results

- IDE diagnostics for all changed Rust files: passed with no diagnostics.
- `git diff --check`: passed.
- `cargo fmt --all -- --check`: blocked (`cargo: command not found`, exit 127).
- `cargo clippy --all-targets --all-features -- -D warnings`: blocked (`cargo: command not found`, exit 127).
- `cargo clippy --workspace --lib -- -D clippy::unwrap_used -D clippy::expect_used`: blocked (`cargo: command not found`, exit 127).
- `cargo test -p bill_payments --test tests_recurring --test tests_bill_schedule_exec --test gas_bench`: blocked (`cargo: command not found`, exit 127).
- `cargo test --workspace`: blocked (`cargo: command not found`, exit 127).
- `cargo build --release --target wasm32-unknown-unknown --workspace`: blocked (`cargo: command not found`, exit 127).
- `bash check_ci.sh`: blocked during its initial lockfile validation because Python is unavailable (exit 49).

The benchmark tests emit `GAS_BENCH_RESULT` records when run in an environment with the Rust toolchain. Their initial baselines are intentionally generous and should be tightened only after stable measurements from the target CI host and protocol version.
