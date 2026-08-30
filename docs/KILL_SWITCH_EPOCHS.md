# Emergency Killswitch: Epochs and Incident Lifecycles

## Overview
An "emergency epoch" refers to the period during which the `emergency_killswitch` contract is in an active pause state. This lifecycle is strictly managed to ensure controlled incident resolution and prevent erratic system behavior.

## Lifecycle of an Emergency Epoch

1. **Epoch Start (Incident)**: The pause admin triggers `pause()`. All guarded operations across modules cease immediately.
2. **Cooling Period (Resolution)**: Once the incident is mitigated, the admin schedules an unpause (`schedule_unpause(timestamp)`) to provide a window for final verification.
3. **Epoch End (Resolution)**:
   - **Normal**: `unpause()` is called by the admin once the ledger timestamp exceeds the scheduled unpause time.
   - **Emergency**: `clear_emergency_state()` is called to lift the pause immediately, bypassing the timelock.

## Example: Managing an Emergency Epoch

```rust
// 1. Epoch Start: Incident detected
killswitch.pause();
assert!(killswitch.is_paused());

// 2. Cooling Period: Schedule unpause for 24 hours later
let now = env.ledger().timestamp();
killswitch.schedule_unpause(&(now + 86_400)); 

// ... time passes ...

// 3. Epoch End: Normal resolution
env.ledger().with_mut(|li| li.timestamp = now + 86_500);
killswitch.unpause();
assert!(!killswitch.is_paused());
```

## Related Documentation
- [Emergency Killswitch Timelock Design](killswitch-timelock.md)
- [Pause Playbook](PAUSE_PLAYBOOK.md)
