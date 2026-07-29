# Operator Management Guide

This document describes how operational roles (operators) are added, rotated, and revoked across the Remitwise smart contract ecosystem.

## Audience
**Operator**: The individual or system responsible for deploying, maintaining, and responding to incidents in the Remitwise contracts.

## 1. Adding an Operator
Operators (such as pause admins or upgrade admins) are typically added by the contract owner after the initial deployment.

### Example: Adding a Pause Admin
```rust
let client = BillPaymentsClient::new(&env, &contract_id);
client.set_pause_admin(&owner_address, &new_pause_admin);
```
- **Requirements**: The caller must have the appropriate authority (e.g., the owner). The contract must not be paused.

## 2. Rotating an Operator
Rotation is the process of transferring privileges from an existing operator address to a new one. This is necessary when personnel change or as a proactive security measure (e.g., key rotation).

### Example: Rotating an Admin
In most Remitwise contracts, rotation is a direct transfer initiated by the current admin:
```rust
let client = RemittanceSplitClient::new(&env, &contract_id);
// The current admin transfers their role to a new address
client.set_pause_admin(&current_admin, &new_admin);
```
- **Validation**: Ensure the new address is securely backed up and has the necessary signers configured before initiating the transfer.

## 3. Revoking an Operator
Revocation permanently removes an operator's access.

### Standard Revocation
To revoke an operator without replacing them (for instance, to permanently disable future upgrades), transfer the role to a provably unspendable or dead address (e.g., `G...DEAD`).
```rust
let dead_address = Address::from_string(&env, &"G...DEAD");
client.set_upgrade_admin(&current_admin, &dead_address);
```

### Emergency Revocation
If an operator key is compromised, use the Global Killswitch to halt operations immediately, preventing the compromised key from executing sensitive actions.
```rust
let killswitch = EmergencyKillswitchClient::new(&env, &killswitch_id);
killswitch.pause(&killswitch_admin);
```
After pausing, the compromised operator must be rotated out using the standard rotation process before unpausing.
