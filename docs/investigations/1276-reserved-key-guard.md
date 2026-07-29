# Issue #1276 — Add tests for the reserved-key guard

## Status: Blocked — target function not found

## Summary
This issue asks for test coverage of a "reserved-key guard": a function
that validates a storage key against a reserved/adjacent/arbitrary
boundary. After an exhaustive search of the codebase, this function
could not be located.

## Investigation

Searched all crates (`bill_payments`, `savings_goals`, `family_wallet`,
`insurance`, `remittance_split`, `reporting`, `orchestrator`,
`emergency_killswitch`, `data_migration`, `remitwise-common`,
`testutils`, `integration_tests`, `scenarios`, `cli`, `benchmarks`)
on latest `upstream/main` for:

- Case-insensitive `reserved` across all `.rs` files
- Common guard naming patterns: `is_reserved_key`, `assert_not_reserved`,
  `RESERVED_KEYS`, `reserved_prefix`, `is_reserved`, `check_key`,
  `validate_key`, `guard_key`, `forbidden`, `prohibited`, `system_key`,
  `internal_key`
- General `Symbol`/key-validation function signatures

## Findings

The only matches for "reserved" in the codebase are:
- A doc comment in `reporting/src/lib.rs:1874` noting a parameter is
  "reserved for future auth scoping" (not a guard)
- Unrelated matches on the substring "preserved" across several test
  files (data preservation assertions, unrelated to key reservation)

No function checking a storage key against a reserved list, prefix, or
range exists anywhere in the current codebase.

## Conclusion

The reserved-key guard described in this issue does not currently exist
in `Remitwise-Contracts`. This may mean:
- The guard was removed or renamed since this issue was filed
- The issue describes intended/planned behavior rather than existing code
- The guard exists under different terminology not covered by the search
  terms above

## Request

Could a maintainer confirm which function this issue refers to, or
whether this issue should be closed/relabeled as a feature request
(implement the guard) rather than a test-coverage task?
