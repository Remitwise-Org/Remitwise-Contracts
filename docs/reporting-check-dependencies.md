# Reporting `check_dependencies` health schema

`ReportingContract::check_dependencies(caller)` is the reporting contract's
admin pre-flight for downstream contract health. It verifies the five addresses
stored in `ContractAddresses` by running lightweight probe calls and returns a
`Vec<DependencyStatus>`.

Use this check before relying on generated reports, dashboards, or alerts that
depend on cross-contract reads.

## Authorization and side effects

- Caller: admin only.
- Authorization: `caller.require_auth()` is enforced before status checks.
- Storage writes: none.
- Normal errors before status output:
  - `ReportingError::NotInitialized` when the reporting contract has not been
    initialized.
  - `ReportingError::Unauthorized` when the caller is not the configured admin.
  - `ReportingError::AddressesNotConfigured` when dependency addresses have not
    been configured.

Because the call is side-effect free, operators can run it before a reporting
job without changing report state or dependency state.

## Output schema

Each returned `DependencyStatus` has three fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `name` | `soroban_sdk::String` | Stable dependency slot name from `ContractAddresses`. |
| `ok` | `bool` | `true` when the lightweight probe call completed successfully. |
| `error_category` | `Option<soroban_sdk::String>` | `None` when `ok` is true; otherwise a stable probe failure category. |

`check_dependencies` returns one status per configured dependency in this fixed
order:

1. `remittance_split`
2. `savings_goals`
3. `bill_payments`
4. `insurance`
5. `family_wallet`

## Status values and probes

The status value is encoded by `ok` plus `error_category`.

| `ok` | `error_category` | Operator meaning |
| --- | --- | --- |
| `true` | `None` | The dependency address responded to the probe method. |
| `false` | `Some(...)` | The dependency probe failed. Treat the dependency as unhealthy until fixed. |

The current dependency probes and failure categories are:

| Dependency `name` | Probe method | Failure category |
| --- | --- | --- |
| `remittance_split` | `try_get_split()` | `get_split_failed` |
| `savings_goals` | `try_get_all_goals(current_contract_address)` | `get_all_goals_failed` |
| `bill_payments` | `try_get_total_unpaid(current_contract_address)` | `get_total_unpaid_failed` |
| `insurance` | `try_get_total_monthly_premium(current_contract_address)` | `get_total_monthly_premium_failed` |
| `family_wallet` | `try_get_owner()` | `get_owner_failed` |

There are no separate string statuses such as `healthy`, `degraded`, or
`unreachable` in the contract output today. Operator tooling should derive those
labels from `ok` and `error_category`.

## Operator guidance

- If every status has `ok == true`, the configured dependencies passed the
  pre-flight. Reports can still return `DataAvailability::Partial` or
  `DataAvailability::Missing` for normal data reasons such as page caps or empty
  dependency result sets.
- If a status has `ok == false`, fix that dependency before trusting reports
  that rely on it. The report builders use direct cross-contract reads, so an
  actually unreachable dependency can fail the report call before a report
  object and `DataAvailability` value are returned.
- If addresses are not configured, `check_dependencies` returns
  `ReportingError::AddressesNotConfigured`; it does not return a status vector.
- If a dependency responds but returns no paginated items, report internals that
  use the shared pagination helper map that empty result to
  `DataAvailability::Missing`.
- If a dependency pagination loop reaches `MAX_DEP_PAGES`, the shared
  pagination helper maps the result to `DataAvailability::Partial`.

## Dependency to report impact

| Dependency | Primary report impact |
| --- | --- |
| `remittance_split` | Remittance summary allocation and financial-health remittance inputs. |
| `savings_goals` | Savings reports, top savings reports, and financial-health savings inputs. |
| `bill_payments` | Bill compliance reports, top bill reports, and financial-health bill inputs. |
| `insurance` | Insurance reports and financial-health insurance inputs. |
| `family_wallet` | Family-wallet owner/configuration health for family-facing reporting flows. |

## Worked example

Scenario: the `insurance` address is configured, but it points at a contract that
does not respond to the expected insurance probe.

`check_dependencies(admin)` returns a status like:

```text
DependencyStatus {
    name: "insurance",
    ok: false,
    error_category: Some("get_total_monthly_premium_failed"),
}
```

Operator response:

1. Treat the insurance dependency as unhealthy.
2. Do not trust insurance or financial-health insurance outputs until the
   configured address is corrected.
3. If a report call is attempted against an actually unreachable dependency, it
   may fail before returning a report-level `DataAvailability`.
4. If the dependency is reachable but returns no policies, the insurance report
   can return `DataAvailability::Missing`.
5. If the dependency is reachable but pagination reaches `MAX_DEP_PAGES`, the
   insurance report can return `DataAvailability::Partial`.

## Alerting rules

Recommended alerting for operator tooling:

- Page on any `ok == false`.
- Include `name` and `error_category` in the alert payload.
- Treat `AddressesNotConfigured`, `NotInitialized`, and `Unauthorized` as
  configuration or access failures, not dependency status values.
- Do not collapse different `error_category` values into one generic error;
  they identify which probe failed.
