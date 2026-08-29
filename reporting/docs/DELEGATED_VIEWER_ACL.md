# Delegated report viewers

Reporting data is private by subject. The owner of a report can read it
directly, and a different address can read it only after the owner grants the
specific report scope. A grant is not a general reporting role and it is not a
permission to act as the owner in another contract.

## Scope model

The ACL currently separates two data classes:

| Scope | Entry point | Data exposed |
| --- | --- | --- |
| `Stored` | `get_stored_report_for` | Full stored financial-health report |
| `Archived` | `get_archived_reports_page_for` | Archived health-score summaries |

Granting `Archived` does not grant `Stored`. The owner and viewer are both
part of the storage key. A grant for Alice and Bob cannot authorize Bob to
read Alice's data by changing the owner argument to Carol.

## Grant lifecycle

The owner calls `grant_viewer(owner, viewer, scope, expires_at)`. The owner
must authenticate the call. An expiry of zero means the grant has no time
limit; a non-zero expiry must be strictly after the current ledger timestamp.
The contract rejects the owner itself, the reporting contract address, and
expired-at-creation grants.

The owner calls `revoke_viewer` to remove the exact `(owner, viewer, scope)`
entry. Removal is immediate for new queries in the same ledger transaction
sequence. Revoking `Stored` leaves an independent `Archived` grant untouched,
and vice versa. Re-granting the same tuple replaces its expiry deliberately;
the latest owner-authenticated decision is the active decision.

## Query authorization

The delegated endpoints authenticate the `viewer` first, then check the
subject, viewer, and scope tuple. The owner bypasses the grant lookup only for
their own subject. A missing, expired, or revoked grant returns the same
`Unauthorized` error as every other denied request.

The contract performs the authorization check before reading the report map or
archive index. This ordering prevents a caller from learning whether a report
exists by comparing a missing-record result with an authorization failure.
The response shape for an authorized empty result is distinct from an
unauthorized error, but only after authorization has succeeded.

## Expiry semantics

An expiring grant is valid while `ledger_timestamp < expires_at`. At the exact
expiry timestamp it is invalid. This boundary avoids an extra readable ledger
at the end of a grant and makes client scheduling deterministic. Expired
entries may remain in the bounded instance map until the owner revokes or
replaces them; the authorization predicate treats them as inactive.

Clients should refresh the ledger timestamp before submitting a read near the
boundary. A successful simulation does not reserve access for a later
transaction. The contract checks the timestamp and grant again during the
actual invocation.

## Adversarial cases

The implementation is designed against these common mistakes:

- passing a different owner in the query than the owner in the grant;
- using a Stored grant to retrieve an Archived page or the reverse;
- reusing a viewer grant after its expiry;
- querying immediately after revocation;
- asking a stranger to inspect another pair's grant state;
- using the reporting contract address as a viewer;
- creating a grant with an expiry at or before the current timestamp;
- inferring report existence from an unauthorized request;
- treating a viewer as if they were the subject in a downstream call.

The viewer is authorized to read only the reporting contract's delegated
entrypoint. The viewer does not receive the owner's signing authority and
cannot grant access onward because `grant_viewer` requires the owner address
to authenticate.

## Compatibility

Existing owner-only query methods retain their signatures and behavior. They
continue to call `user.require_auth()`, so existing clients do not silently
become public readers. New integrations that need delegation should use the
explicit `*_for` methods and pass the viewer as the authenticated caller.

The ACL uses a new instance-storage key, `VIEW_ACL`, and does not reinterpret
existing report or archive keys. Deployments therefore do not need to migrate
existing report records. The new key is initialized lazily on the first grant
or revoke operation.

The new `ReportScope` and `ViewerGrant` values are contract types. Indexers
should decode them by their discriminants and preserve the owner, viewer,
scope, and expiry from the grant events. They should not infer scope from an
entrypoint name alone because future releases may add another explicitly
versioned scope.

## Indexing and audit

`ViewerGranted` records the complete grant tuple and expiry. `ViewerRevoked`
records the tuple with expiry zero to identify the exact permission removed.
Indexers should key these records by ledger sequence and transaction hash,
then maintain the latest state for each tuple. Event replay must be
idempotent.

An indexer must not turn a grant event into a general “user has reporting
access” flag. Scope is part of the authorization decision. A viewer with an
Archived grant is still unauthorized for stored reports, even if the indexer
has seen a previous grant for the same owner and address.

Operational dashboards should show active, expired, and revoked grants
separately. Expired grants are useful audit evidence but must not be presented
as active permissions. Revoke events should remain visible after a later grant
so that the permission history is reconstructable.

## Incident response

If a viewer key is compromised, the owner should revoke each active scope for
that viewer. Revocation is a contract state change and must be confirmed on
the target network. Rotating a viewer address off-chain is not a substitute
for revocation.

If the owner key is compromised, the reporting contract's admin rotation and
the owner's broader account recovery process must be followed. The admin is
not a fallback viewer and cannot read a user's reports through these methods
without an explicit grant from that user.

If an indexer shows a viewer as active after revocation, query the contract's
grant state and transaction history. Do not rely on the stale index to make a
privacy decision. The on-chain authorization predicate is authoritative.

## Testing matrix

The focused test suite covers:

1. Stored access is granted only to the exact owner/viewer tuple.
2. Stored access does not imply Archived access.
3. A grant cannot cross owners.
4. Revocation blocks the next query.
5. Expiry blocks access at the exact boundary timestamp.
6. Invalid expiry values are rejected.
7. Self-viewer grants are rejected.
8. The owner can read without a grant.
9. An unrelated caller cannot inspect grant state.

Repository-level authorization tests continue to exercise the existing
owner-only query matrix. Gas checks should include both delegated endpoints,
the grant write, and the revoke write, with representative existing map sizes.

## Rollback and release

The change is additive for storage and API surface. If a deployment must be
rolled back, the previous contract version can continue to read the existing
report and archive keys; the ACL key is ignored by that version. Operators
should nevertheless revoke sensitive viewer grants before rollback when the
rollback version does not enforce the new delegated methods, and should record
that compatibility decision in the release manifest.

Before release, verify the complete matrix on a disposable network, inspect
grant and revoke events, test the exact expiry boundary, and confirm that no
unauthorized response reveals whether a stored or archived report exists.
