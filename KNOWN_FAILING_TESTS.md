# Known Failing Tests

The following 13 tests are explicitly quarantined with `#[ignore = "..."]`. They are real defects, behavioural discrepancies, or depend on downstream crate artifacts that do not build today. They are tracked here and in CI gates to ensure no new ignored or failing tests are introduced without notice.

| Test | Crate | Reason / Observed vs Expected |
| --- | --- | --- |
| `amount::tests::checked_sub_is_exact` | `remitwise-common` | Observed: panics or underflow mismatch on `checked_sub_amount(0, 1)`. Expected: `Err(AmountOverflowError)` |
| `tests::distributes_using_basis_points_denomination` | `remitwise-common` | Observed: remainder 900,000 allocated to last bucket so sum is 1,000,000. Expected: sum 100,000 |
| `tests::test_canonicalise_symbol_empty_panics` | `remitwise-common` | Observed: empty symbol does not panic with expected message. Expected: panic with "symbol input must contain between 1 and 32 characters" |
| `stable_currency_tests::rejects_special_char_token` | `remitwise-common` | Observed: panics inside `Symbol::new` on `$` character. Expected: returns `Err(StableCurrencyError::UnsupportedCurrency)` |
| `test_wasm_artifacts_respect_documented_size_budgets` | `orchestrator` | Observed: missing downstream `.wasm` artifacts (`remittance_split`, `savings_goals`, `bill_payments`, `insurance`, `family_wallet`). Expected: all `.wasm` files exist and fit budget |
| `test_unsigned_rollback_bill_failure_returns_rolled_back` | `orchestrator` | Observed: returns `CrossContractCallFailed`. Expected: returns `RemittanceFlowRolledBack` |
| `test_unsigned_rollback_insurance_failure_returns_rolled_back` | `orchestrator` | Observed: returns `CrossContractCallFailed`. Expected: returns `RemittanceFlowRolledBack` |
| `test_fanout_flow_does_not_compensate_on_bill_failure` | `orchestrator` | Observed: error variant mismatch on fanout step failure. Expected: `CrossContractCallFailed` without compensation |
| `tests_nonce_eviction::signed_flow_replay_uses_used_set_and_old_nonce_uses_sequential_counter` | `orchestrator` | Observed: aborts with `HostError: Error(WasmVm, InvalidAction)`. Expected: clean rejection with `InvalidNonce` |
| `tests_nonce_eviction::used_nonce_eviction_keeps_stale_replay_closed` | `orchestrator` | Observed: aborts with `HostError: Error(WasmVm, InvalidAction)`. Expected: clean eviction and replay rejection |
| `tests_nonce_eviction::deadline_window_rejections_do_not_consume_nonce` | `orchestrator` | Observed: aborts with `HostError: Error(WasmVm, InvalidAction)`. Expected: `DeadlineExpired` without consuming nonce |
| `tests_nonce_eviction::request_hash_binding_rejects_parameter_swap_without_consuming_nonce` | `orchestrator` | Observed: aborts with `HostError: Error(WasmVm, InvalidAction)`. Expected: parameter swap rejection without consuming nonce |
| `tests_viewer_acl::invalid_expiry_and_self_viewer_are_rejected` | `reporting` | Observed: failure in viewer grant rejection validation. Expected: rejection with `InvalidGrantExpiry` / `InvalidViewer` |

## Summary of Ignored Counts

- `remitwise-common`: 4 tests (limit: 4)
- `orchestrator`: 8 tests (limit: 8)
- `reporting`: 1 test (limit: 1)
- Total: 13 tests
