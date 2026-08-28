# USDC Distribution Security Model

This note describes the authorization and asset-identity guarantees for the
USDC distribution entrypoints. It is intentionally separate from the split
math documentation: correct percentages do not, by themselves, prove that a
transfer was authorized or that it used the configured asset.

## Trusted configuration

`initialize_split` stores the owner and the USDC contract address together in
`SplitConfig`. The asset address is not accepted as a trusted value on each
call. Every later distribution compares the supplied asset with the pinned
configuration and returns `UntrustedTokenContract` before constructing a
token transfer when they differ.

The configuration is instance storage and cannot be replaced by calling
`initialize_split` again. An update to split percentages does not accept an
asset argument, so changing percentages cannot substitute the token contract.

## Authorization layers

The distribution checks are deliberately layered:

1. `from.require_auth()` proves that the source account authorized the
   transaction at the Stellar authorization layer.
2. The source account must equal `SplitConfig.owner`.
3. The token address must equal `SplitConfig.usdc_contract`.
4. Destination self-transfers are rejected.
5. The nonce and deadline must be valid before token calls.

The structured `distribute_usdc_hashed` path applies the owner check after
authentication and configuration loading, before nonce mutation or token
transfers. A valid signature from a different funded account therefore cannot
turn that account into an owner-authorized distribution caller.

The legacy `distribute_usdc` path retains its existing argument and hash
format for compatibility and applies the same owner and asset checks. New
integrations that need cryptographic binding of every request field should use
`distribute_usdc_hashed` with `DistributeUsdcRequest` and its SHA-256 hash.

## Request binding

`get_request_hash` binds the distribution domain, payer, configured asset,
all four destination accounts, total amount, nonce, and deadline. Mutating any
of these fields while retaining the original hash returns
`RequestHashMismatch`. This prevents an intermediary from replacing a
recipient or amount after a signer approved the request.

The hash is checked before `from.require_auth()` and before configuration is
used for any token transfer. The owner and configured-asset comparisons still
run independently; request hashing is an additional binding, not a substitute
for authorization or configuration validation.

## Batch distribution safety

`batch_transfer` is owner-directed and uses the immutable configured asset.
It validates the following before the first transfer:

- `recipients.len() == amounts.len()`;
- the number of recipients is at most `MAX_BATCH_SIZE`;
- every amount is strictly positive;
- the caller is the configured owner and has authorized the call;
- the nonce is unused and equals the expected value.

The loop uses the same index for the two vectors, so no amount can be paired
with a different recipient. A vector length or amount error leaves the nonce
and every balance unchanged.

## Replay and failure atomicity

Successful distributions advance the source nonce only after all token
transfers complete. Reusing a successful nonce returns `NonceAlreadyUsed` and
cannot issue a second payout. If a token transfer fails, Soroban rolls back
the whole invocation: earlier transfers in the same call, the nonce mutation,
audit records, and emitted events are not committed.

This ordering matters for both the four-way split and arbitrary batch
distribution. It means the observable state is either the complete successful
distribution or the exact pre-call state.

## Failure matrix

| Adversarial input | Guard | State effect |
|---|---|---|
| Non-owner source | `Unauthorized` | No token call; nonce unchanged |
| Wrong token contract | `UntrustedTokenContract` | No token call; nonce unchanged |
| Changed signed field | `RequestHashMismatch` | No token call; nonce unchanged |
| Expired deadline | `DeadlineExpired` | No token call; nonce unchanged |
| Reused nonce | `NonceAlreadyUsed` | No token call; nonce unchanged |
| Mismatched batch vectors | `BatchLengthMismatch` | No token call; nonce unchanged |
| Oversized batch | `BatchSizeExceeded` | No token call; nonce unchanged |
| Token transfer failure | token error + rollback | All balances and nonce restored |

## Compatibility and migration

No existing entrypoint was removed or its argument order changed. Existing
callers of `distribute_usdc` and `batch_transfer` retain their interface.
The stronger structured request flow is additive. Deployments upgrading from
an older configuration must retain the existing `SplitConfig.usdc_contract`
value; changing that value requires an explicit contract migration rather than
a caller-supplied substitution.
