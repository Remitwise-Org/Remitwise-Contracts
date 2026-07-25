# Verifier Registry Workflow

**Target Audience:** Operators & Infrastructure Maintainers

This document codifies the "tribal knowledge" surrounding how Ed25519 public keys (Verifiers) are managed, rotated, and revoked across the Remitwise infrastructure. 

While the core smart contracts use the `remitwise_common::verify_signature` utility to authenticate signed payloads (e.g., in the Orchestrator), the actual lifecycle management of the authorized public keys—the "Verifier Registry"—happens at the operational and configuration level.

## 1. What is a Verifier?

In the Remitwise ecosystem, a **Verifier** is a 32-byte Ed25519 public key. 
Backend services use the corresponding private key to sign execution requests (like `execute_remittance_flow_signed`), and the smart contracts or off-chain indexers verify these signatures to ensure authenticity and prevent tampering.

## 2. Adding a New Verifier

When a new backend service or authorized relayer is provisioned, a new Ed25519 keypair must be generated and registered.

### Step 2.1: Key Generation
Operators should generate a new standard Ed25519 keypair using the Soroban CLI:

```bash
soroban keys generate backend-relayer-02
```

### Step 2.2: Registration
The public key must be added to the environment configuration of the Orchestrator/API gateway. (Currently, this registry is maintained off-chain in the infrastructure configuration, preventing the need for costly on-chain storage rent for off-chain services).

```bash
# Example operational request to update the active verifiers
export REMITWISE_ACTIVE_VERIFIERS="G...,G...,<NEW_PUBLIC_KEY>"
```

## 3. Rotating a Verifier

Verifier rotation is a proactive security measure. A verifier should be rotated periodically (e.g., every 90 days) or immediately if a backend instance is replaced.

To ensure zero-downtime, rotation is a two-step process:

1. **Add the New Key:** Generate and append the new verifier public key to the active registry configuration (as shown in Step 2). Both the old and new keys are now valid.
2. **Deploy the Private Key:** Update the backend service to begin signing new payloads using the *new* private key.
3. **Verify:** Confirm logs show successful `verify_signature` checks using the new key.
4. **Revoke the Old Key:** Proceed to Step 4 to remove the old key.

## 4. Revoking a Verifier

Revocation is required when a key is compromised, a service is decommissioned, or a scheduled rotation is completed.

### Step 4.1: Configuration Update
Remove the public key from the active verifier registry configuration. 

```bash
# Remove the old/compromised key from the allowed list
export REMITWISE_ACTIVE_VERIFIERS="<ONLY_VALID_KEYS>"
```

### Step 4.2: Audit and Purge
Once revoked, any inflight transactions signed by the revoked key will immediately fail the `verify_signature` preflight and be rejected by the orchestrator. 

Operators should confirm the revocation by searching the logs for `SignatureError::VerificationFailed` associated with the revoked public key:

```bash
# Example log query to confirm rejection
grep "VerificationFailed" /var/log/remitwise/orchestrator.log
```

---
*Note: Future roadmap items include migrating this off-chain operational registry into a dedicated `VerifierRegistry` smart contract for on-chain authorization. When implemented, this document will be updated to reflect the new `add_verifier` and `revoke_verifier` entrypoints.*
