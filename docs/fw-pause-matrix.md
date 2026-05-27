# Family Wallet Pause-Coverage Matrix

This document provides a comprehensive audit matrix confirming that every state-mutating entrypoint in the `FamilyWallet` smart contract correctly honors the pause flag.

For security and operational integrity, all state-mutating functions check `is_paused` and reject when paused, with the explicit exception of pause administration tools (`pause`, `unpause`, and `set_pause_admin`) which must remain callable to manage the contract's lifecycle.

## Pause Protection Matrix

| Entrypoint | Mutating? | Checks Pause? | Access Control | Description / Context |
| :--- | :---: | :---: | :--- | :--- |
| `init` | Yes | No | Owner | Contract initialization. Pause state is uninitialized (false) at deployment. |
| `add_member` | Yes | **Yes** | Owner / Admin | Adds a member with custom spending limit. |
| `get_member` | No | No | Public | Query family member details. |
| `update_spending_limit` | Yes | **Yes** | Owner / Admin | Updates spending limit for a member. |
| `check_spending_limit` | No | No | Public | Queries spending limit validation. |
| `validate_precision_spending` | No | No | Public | Queries precision limit validation. |
| `configure_multisig` | Yes | **Yes** | Owner / Admin | Configures multi-sig signature thresholds and signers list. |
| `propose_transaction` | Yes | **Yes** | Owner / Admin | Proposes a multisig transaction. |
| `sign_transaction` | Yes | **Yes** | Signers | Signs/approves a proposed multisig transaction. |
| `withdraw` | Yes | **Yes** | Members | Initiates/proposes a withdrawal. |
| `propose_split_config_change` | Yes | **Yes** | Members | Proposes a change to the remittance split percentages. |
| `propose_role_change` | Yes | **Yes** | Members | Proposes a family member role change. |
| `propose_emergency_transfer` | Yes | **Yes** | Members | Proposes/executes an emergency transfer. |
| `propose_policy_cancellation` | Yes | **Yes** | Members | Proposes the cancellation of a micro-insurance policy. |
| `configure_emergency` | Yes | **Yes** | Owner / Admin | Configures emergency transfer limits/cooldowns. |
| `set_emergency_mode` | Yes | **Yes** | Owner / Admin | Enables/disables emergency mode. |
| `add_family_member` | Yes | **Yes** | Owner / Admin | Adds a member with spending limit of 0. |
| `remove_family_member` | Yes | **Yes** | Owner | Removes a member from the wallet. |
| `get_pending_transaction` | No | No | Public | Queries details of a pending transaction. |
| `get_pending_transactions_page` | No | No | Public | Paginated query of pending transactions. |
| `get_multisig_config` | No | No | Public | Queries multisig configuration. |
| `get_family_member` | No | No | Public | Queries member data. |
| `get_owner` | No | No | Public | Queries owner address. |
| `get_emergency_config` | No | No | Public | Queries emergency transfer configurations. |
| `is_emergency_mode` | No | No | Public | Queries emergency mode status. |
| `get_last_emergency_at` | No | No | Public | Queries last emergency transfer timestamp. |
| `archive_old_transactions` | Yes | **Yes** | Owner / Admin | Archives executed transactions. |
| `get_archived_transactions` | No | No | Owner / Admin | Queries archived transactions. |
| `cleanup_expired_pending` | Yes | **Yes** | Owner / Admin | Removes expired proposals from storage. |
| `get_storage_stats` | No | No | Public | Queries storage stats. |
| `set_role_expiry` | Yes | **Yes** | Owner / Admin | Sets role-expiry timestamp for a member. |
| `get_role_expiry_public` | No | No | Public | Queries role-expiry for a member. |
| `set_precision_spending_limit` | Yes | **Yes** | Owner / Admin | Sets withdrawal precision limits for a member. |
| `get_spending_tracker` | No | No | Public | Queries cumulative spending tracker. |
| `cancel_transaction` | Yes | **Yes** | Proposer / Admin | Cancels a pending proposal. |
| `pause` | Yes | No | Pause Admin | Pauses the contract. Must bypass pause check to allow pausing. |
| `unpause` | Yes | No | Pause Admin | Unpauses the contract. Must bypass pause check to allow unpausing. |
| `set_pause_admin` | Yes | No | Owner | Sets a new pause admin. Must remain mutable during pause. |
| `is_paused` | No | No | Public | Queries contract pause state. |
| `get_version` | No | No | Public | Queries contract version. |
| `set_proposal_expiry` | Yes | **Yes** | Owner | Sets the proposal expiry window. |
| `get_proposal_expiry_public` | No | No | Public | Queries proposal expiry window. |
| `set_upgrade_admin` | Yes | **Yes** | Owner | Sets the upgrade admin address. |
| `get_upgrade_admin_public` | No | No | Public | Queries upgrade admin address. |
| `set_version` | Yes | **Yes** | Upgrade Admin | Updates contract version (upgrade support). |
| `batch_add_family_members` | Yes | **Yes** | Owner / Admin | Adds multiple family members at once. |
| `batch_remove_family_members` | Yes | **Yes** | Owner | Removes multiple family members at once. |
| `get_access_audit` | No | No | Public | Queries access audit log. |
| `get_access_audit_page` | No | No | Public | Paginated query of access audit log. |
