# Soroban Gas Benchmarking - Family Wallet

Gas benchmarks for the `family_wallet` contract to monitor performance on multisig and emergency paths.

## Methodology
Benchmarks are executed in a controlled Soroban environment with a reset budget. We measure:
- **CPU Instructions**: Computational complexity of the operation.
- **Memory Bytes**: Memory overhead of the transaction.

## How to Run
Execute the following command to run the gas benchmarks and view the results:
```bash
RUST_TEST_THREADS=1 cargo test -p family_wallet --test gas_bench -- --nocapture
```

## Benchmark Scenarios

| Method | Scenario | Goal |
|--------|----------|------|
| `init` | 5 Initial Members | Measure cost of baseline setup. |
| `propose_transaction` | Large Withdrawal | Cost of creating a pending multisig proposal. |
| `sign_transaction` | Non-Executing | Cost of adding a signature (storage write). |
| `sign_transaction` | Executing | Cost of the final signature + cross-contract execution. |
| `emergency_transfer` | Direct Execute | Performance of emergency path when mode is active. |
| `archive_transactions` | 10 Executed TXs | Bulk cleanup and archival to long-term storage. |
| `cleanup_expired_pending` | 10 Expired TXs | Performance of scanning and removing expired proposals. |

## Baseline Results (Protocol 22)

| Method | Scenario | CPU Instructions | Memory Bytes |
|--------|----------|------------------|--------------|
| `init` | 5 Initial Members | 111,193 | 19,048 |
| `propose_transaction` | Large Withdrawal | 233,380 | 46,583 |
| `sign_transaction` | Non-Executing (1 of 3) | 268,848 | 55,278 |
| `sign_transaction` | Executing (2 of 2) | 422,826 | 78,512 |
| `propose_emergency_transfer` | Direct Execute (Mode ON) | 395,269 | 67,938 |
| `archive_old_transactions` | 10 Executed TXs | 390,675 | 90,594 |
| `cleanup_expired_pending` | 10 Expired TXs | 617,140 | 127,538 |
| `batch_add_family_members` | 20 New Members | 556,534 | 101,502 |

## Security Notes
- **Authorization**: All benchmarks verify that `require_auth` is properly called.
- **Spending Limits**: Benchmarks include scenarios where spending limits are checked.
- **Emergency Cooldowns**: Verified that cooldown logic is enforced even in benchmarks.
