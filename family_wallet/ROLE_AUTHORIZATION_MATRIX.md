# Family Wallet Authorization Matrix

The serialized role set in `remitwise-common` is `Owner`, `Admin`, `Member`, and `Viewer`. `Viewer` is read-only; `Member` is the configured spending role. Authorization is evaluated from current storage state on every operation.

| Entrypoint group | Owner | Admin | Member | Viewer |
| --- | --- | --- | --- | --- |
| Read member/configuration data | Yes | Yes | Public/query-specific | Public/query-specific |
| Spend or propose withdrawal/emergency transfer | Yes | Yes | Yes, within configured limit | No |
| Sign a configured transaction | Yes, if configured | Yes, if configured | Yes, if configured | No |
| Add members and configure spending/multisig | Yes | Yes | No | No |
| Set role expiry or pause | Yes | Yes, subject to pause-admin rules | No | No |
| Remove members, upgrade, proposal-expiry policy | Yes | No | No | No |
| Propose role changes | Yes | Yes | No | No |

Security invariants:

- A role change cannot assign `Owner`, and only an active `Owner` or `Admin` can propose one.
- Spending entrypoints reject viewers, removed members, and expired roles before creating proposal state or updating emergency accounting.
- Multisig execution revalidates the proposer against current membership before any token or transaction-state mutation, preventing removed-role replay.
- Member removal revalidates pending proposals and strips removed signatures.
