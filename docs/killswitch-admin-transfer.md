# Emergency Killswitch Admin Transfer

## Overview

`emergency_killswitch::transfer_admin(new_admin)` rotates the single address stored at
`DataKey::Admin`. The stored admin controls global pause, scheduled unpause,
module pause, and per-function pause operations, so admin transfer is treated as
an emergency control-plane operation.

## Authorization Flow

1. `initialize(admin)` must have completed first. If `DataKey::Admin` is absent,
   `transfer_admin` returns `Error::NotInitialized`.
2. `transfer_admin` loads the current `DataKey::Admin`.
3. The current admin must authorize the call through `admin.require_auth()`.
4. After authorization succeeds, `DataKey::Admin` is overwritten with
   `new_admin`.

Soroban auth failures occur at the host layer. A caller that cannot satisfy the
current admin's `require_auth()` does not receive a contract-level
`Error::Unauthorized`; the call fails with an auth `HostError`. Contract-level
`Error::Unauthorized` remains used by logic checks such as attempting to unpause
before the scheduled timestamp.

## Security Properties

- Only the address currently stored at `DataKey::Admin` can authorize a
  successful admin rotation.
- The previous admin loses all pause, unpause, and module-pause authority as
  soon as `DataKey::Admin` is updated.
- Transferring to the same address is deterministic: the stored admin remains
  unchanged, and the admin keeps pause authority.
- Chained transfers are single-owner at every step. For `A -> B -> C`, `B` loses
  authority once `C` is stored.

## Verification

The unit tests in `emergency_killswitch/src/lib.rs` assert:

- transfer before initialization returns `Error::NotInitialized`;
- non-admin auth cannot satisfy `transfer_admin`;
- successful transfer updates `DataKey::Admin`;
- the new admin can `pause`, `schedule_unpause`, `unpause`, and `pause_module`;
- the old admin cannot `pause`, `unpause`, or `pause_module`;
- self-transfer and double-transfer behavior are deterministic.
