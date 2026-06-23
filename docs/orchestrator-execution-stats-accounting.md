# Orchestrator execution statistics — accounting model

`Orchestrator::get_execution_stats` exposes an `ExecutionStats` structure maintained by `execute_remittance_flow_signed`.

## Invariant accounting identity

After any number of signed flow attempts that reach the point where stats are updated, the following identity must hold:

`total_executions == successful_executions + failed_executions`

Where:

- `total_executions` is incremented exactly once per execution attempt.
- `successful_executions` increments only when the internal execution path returns `Ok(_)`.
- `failed_executions` increments only when the internal execution path returns `Err(_)`.

## Pre-execution rejections must not mutate stats

A signed request can be rejected **before** the internal execution path runs (e.g. because of authorization, invalid amount, deadline/nonce validation, or execution lock already held).

For these pre-execution rejections, `ExecutionStats` MUST NOT be mutated.

Concretely, there must be no counter inflation when:

- deadline is expired / invalid
- nonce is invalid / out of order
- nonce is already used
- `EXEC_LOCK` is already set

## Lock consistency

`execute_remittance_flow_signed` sets `EXEC_LOCK` before executing the flow and clears it on all non-panicking paths.

- On successful execution: lock is cleared and stats are updated as `success`.
- On internal execution error: lock is cleared and stats are updated as `failure`.
- On panic: Soroban rolls back the whole transaction; tests may observe lock state restored.
