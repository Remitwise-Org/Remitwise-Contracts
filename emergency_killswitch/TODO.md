# #1292 Add `require_matching_kill_switch_epoch(ep)` guard

## Task Progress

### Analysis
- [x] Understand repository structure
- [x] Identify the `emergency_killswitch` crate as the target
- [x] Review existing contract code (lib.rs, tests, docs)

### Implementation
- [x] Add `Error::EpochMismatch = 7` to the error enum
- [x] Add `DataKey::KillSwitchEpoch` to storage keys
- [x] Initialize epoch to `0` in `initialize()`
- [x] Add `require_matching_kill_switch_epoch(Env, ep)` guard function
- [x] Add `bump_kill_switch_epoch(Env, caller)` admin function with event
- [x] Add `get_kill_switch_epoch(Env)` query function
- [x] Update `transfer_admin(Env, new_admin, ep)` to require epoch match

### Tests
- [x] Update existing tests to pass epoch parameter to `transfer_admin`
- [x] Add negative test: `test_transfer_admin_wrong_epoch_rejected`
- [x] Add negative test: `test_stale_epoch_rejected_after_bump`
- [x] Add positive test: `test_get_kill_switch_epoch_after_initialize`
- [x] Add positive test: `test_get_kill_switch_epoch_before_initialize`
- [x] Add positive test: `test_require_matching_kill_switch_epoch_ok`
- [x] Add negative test: `test_require_matching_kill_switch_epoch_fails`
- [x] Add negative test: `test_bump_kill_switch_epoch_not_initialized`
- [x] Add negative test: `test_bump_kill_switch_epoch_unauthorized_caller`

### Documentation
- [ ] Update PR description with threat model
- [ ] Reference issue #1292 with "Closes #"

