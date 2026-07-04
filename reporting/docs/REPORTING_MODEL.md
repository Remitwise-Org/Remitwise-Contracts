# Reporting Model

This document is the integrator-facing reference for the `reporting` contract's
financial-health scoring and `DataAvailability` semantics.

Source of truth: `reporting/src/lib.rs`.

## HealthScore Weights

`calculate_health_score(user, total_remittance)` returns a `HealthScore` with
one total score and three component scores:

| Component | Range | Source input | Rule |
| --- | ---: | --- | --- |
| `savings_score` | 0-40 | `savings_goals.get_all_goals(user)` | Aggregate `current_amount / target_amount`, scaled to 40 points. |
| `bills_score` | 0-40 | `bill_payments.get_unpaid_bills(user, 0, 1000)` | Tiered score: 40 if no unpaid bills, 35 if unpaid bills exist but none are overdue, 20 if any unpaid bill is overdue. |
| `insurance_score` | 0-20 | `insurance.get_active_policies(user, 0, 1)` | Binary score: 20 when at least one active policy exists, otherwise 0. |
| `score` | 0-100 | Component sum | `clamp(savings_score + bills_score + insurance_score, 0, 100)`. |

The three maximum component weights add to 100, so the final clamp is a
defensive bound. Under normal operation, `score` equals the component sum.

The `total_remittance` argument is currently kept for API stability and is not
used by the health-score calculation.

## Savings Component

The savings component walks every returned savings goal and clamps each goal
before summing:

- `target = clamp(goal.target_amount, 0, i128::MAX / 2)`
- `saved = clamp(goal.current_amount, 0, target)`
- totals use saturating addition

If `total_target == 0`, the savings component returns the neutral default of
20 points. Otherwise:

```text
progress = min((total_saved * 100) / total_target, 100)
savings_score = min((progress * 40) / 100, 40)
```

Examples:

| Aggregate savings progress | `savings_score` |
| ---: | ---: |
| no targets | 20 |
| 0% | 0 |
| 50% | 20 |
| 80% | 32 |
| 100% or more | 40 |

## Bills Component

The bills component inspects up to 1000 unpaid bills.

| Condition | `bills_score` |
| --- | ---: |
| No unpaid bills | 40 |
| Unpaid bills exist, none overdue | 35 |
| At least one unpaid bill is overdue | 20 |

A bill is overdue when `bill.due_date < env.ledger().timestamp()`.

This component is intentionally tiered. It does not scale by the number or value
of unpaid bills.

## Insurance Component

The insurance component fetches one active-policy page and checks only whether
any active policy ID exists.

| Condition | `insurance_score` |
| --- | ---: |
| At least one active policy exists | 20 |
| No active policy exists | 0 |

Coverage amount, premium, and coverage-to-premium ratio are exposed by
`InsuranceReport`, but they do not affect `insurance_score`.

## FinancialHealthReport Roll-up

`get_financial_health_report` combines:

- `health_score`
- `remittance_summary`
- `savings_report`
- `bill_compliance`
- `insurance_report`
- a top-level `data_availability`
- `generated_at`

The top-level `data_availability` is the worst value across:

1. `remittance_summary.data_availability`
2. `bill_compliance.data_availability`
3. `insurance_report.data_availability`

`SavingsReport` does not currently expose `DataAvailability`, so it does not
participate in the roll-up.

Worst-value order:

```text
Missing > Partial > Complete
```

That means:

| Inputs | Top-level availability |
| --- | --- |
| all `Complete` | `Complete` |
| any `Partial`, no `Missing` | `Partial` |
| any `Missing` | `Missing` |

## DataAvailability Semantics

`DataAvailability` describes whether a report's cross-contract data source was
complete enough for the returned aggregate:

| Value | Meaning | Typical cause |
| --- | --- | --- |
| `Complete` | The dependency data was read fully within the configured page cap. | Pagination reached `next_cursor == 0`. |
| `Partial` | The report was computed from a truncated or degraded dependency read. | Pagination reached `MAX_DEP_PAGES`; a mid-pagination/member read failed after earlier data; remittance split read failed and allocation details were omitted. |
| `Missing` | The report has no usable dependency data for the requested aggregate. | Addresses are absent for remittance summary, the first dependency page is empty in shared pagination, or a critical first-page family-wallet read fails. |

The shared dependency pagination cap is:

```text
MAX_DEP_PAGES = 20
DEP_PAGE_LIMIT = 50
```

So bill and insurance report builders can inspect at most 1000 dependency items
per call before marking the result `Partial`.

## HealthScore and Availability

`HealthScore` itself does not contain a `DataAvailability` field and does not
consult the report-level availability flags. It directly reads the configured
dependencies and returns the component defaults when those dependencies return
ordinary empty result sets:

- no savings goals -> savings score 20
- no unpaid bills -> bills score 40
- no active policies -> insurance score 0

If `ContractAddresses` are not configured, `calculate_health_score` returns
`ReportingError::AddressesNotConfigured`. If a direct cross-contract call
fails, the call fails instead of returning a partial `HealthScore`.

Consumers that need degradation metadata should call
`get_financial_health_report` or the dedicated report endpoints and inspect
their `DataAvailability` fields.

## Worked Examples

| Scenario | Savings | Bills | Insurance | Total | Availability note |
| --- | ---: | ---: | ---: | ---: | --- |
| Healthy user: 80% savings progress, unpaid bills but none overdue, active policy | 32 | 35 | 20 | 87 | Availability depends on report endpoint reads. |
| Brand-new user: no goals, no unpaid bills, no active policy | 20 | 40 | 0 | 60 | Empty inputs are normal defaults for `HealthScore`. |
| At-risk user: 0% savings, overdue unpaid bill, no policy | 0 | 20 | 0 | 20 | Availability can still be `Complete` if dependencies were fully read. |
| Bill pagination reaches the page cap | unchanged by bill report pagination | unchanged by bill report pagination | unchanged | component sum | `BillComplianceReport` and the top-level report become `Partial`. |
| Insurance dependency has no usable first page | component may be 0 or fail depending on the direct score call | unchanged | 0 if direct empty page | component sum | `InsuranceReport` and the top-level report become `Missing`. |

## Related Documents

- Root health-score reference: `docs/HEALTH_SCORE.md`
- Dependency pre-flight schema: `docs/reporting-check-dependencies.md`
- Family-wallet report availability rules: `reporting/docs/FAMILY_SPENDING_REPORT.md`
