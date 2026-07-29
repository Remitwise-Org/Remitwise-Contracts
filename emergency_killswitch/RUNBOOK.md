# Emergency Killswitch Runbook

## Admin Transfer

Use `transfer_admin(new_admin)` only when the replacement admin account is ready
to operate the emergency controls. The current admin must authorize the
transaction; no other address can rotate `DataKey::Admin`.

Operational steps:

1. Confirm the replacement address is controlled by the intended multisig or
   hardware-backed account.
2. Submit `transfer_admin(new_admin)` from the current admin.
3. Verify `DataKey::Admin` now resolves to `new_admin`.
4. Run a controlled pause-flow check in a test or staging environment:
   `pause`, `schedule_unpause`, `unpause`, and `pause_module`.
5. Treat the old admin as revoked immediately after the transaction succeeds.

Expected failure behavior:

- Before initialization, `transfer_admin` returns `Error::NotInitialized`.
- If the current admin does not authorize the call, Soroban rejects the
  transaction with an auth `HostError`.
- After transfer, any pause or unpause operation authorized by the old admin
  fails at the Soroban auth layer.
