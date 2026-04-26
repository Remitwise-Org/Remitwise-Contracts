# RemitWise `distribute_usdc` Request Hash Signer Guide

This document provides a comprehensive guide for integrators and signers who need to sign RemitWise `distribute_usdc` requests using the typed request-hash helpers introduced in SC-001.

## Overview

The `distribute_usdc` function now supports secure, deterministic request signing through the `get_request_hash()` helper API. This allows off-chain signers to cryptographically authorize USDC distribution requests without ambiguity or parameter tampering risks.

## Key Concepts

### Request Hash (32 bytes)

A canonical SHA-256 hash computed from a `DistributeUsdcRequest` structure. This hash:
- Is **deterministic**: Same inputs always produce the same output
- Is **parameter-bound**: Every parameter is cryptographically included in the hash
- Is **domain-separated**: Includes "distribute_usdc_v1" to prevent cross-version attacks
- Is **verifiable**: Can be independently computed by signers and the contract

### Signer Workflow

The typical flow for authorizing a USDC distribution:

```
1. Integrator constructs DistributeUsdcRequest with all parameters
   ├─ usdc_contract: USDC token contract address
   ├─ from: Payer/sender address
   ├─ nonce: Current nonce for replay protection
   ├─ accounts: Destination AccountGroup (spending, savings, bills, insurance)
   ├─ total_amount: Total USDC to distribute
   └─ deadline: Expiry timestamp (Unix seconds)

2. Integrator calls get_request_hash(request) → hash (32 bytes)

3. Off-chain signer signs the hash with their private key
   └─ Signature proves intent to authorize this exact request

4. Integrator submits transaction:
   - distribute_usdc_with_hash_and_deadline(request, hash)
   
5. Contract verifies:
   ├─ compute_request_hash(request) == provided_hash ✓
   ├─ current_time <= request.deadline ✓
   ├─ request.nonce == expected_nonce ✓
   └─ Then executes USDC transfers
```

## Parameter Binding

All parameters are cryptographically bound through the SHA-256 hash:

### Mandatory Parameters

| Field | Type | Purpose | Security Risk if Tampered |
|-------|------|---------|---------------------------|
| `usdc_contract` | Address | USDC token contract address | Attacker could redirect to different token contract |
| `from` | Address | Payer address | Attacker could impersonate the signer |
| `nonce` | u64 | Transaction sequence number | Attacker could replay old authorized requests |
| `accounts.spending` | Address | Spending destination | Attacker could redirect funds to their account |
| `accounts.savings` | Address | Savings destination | Attacker could redirect funds to their account |
| `accounts.bills` | Address | Bills destination | Attacker could redirect funds to their account |
| `accounts.insurance` | Address | Insurance destination | Attacker could redirect funds to their account |
| `total_amount` | i128 | Total USDC to distribute | Attacker could increase distribution amount |
| `deadline` | u64 | Request expiry time | Attacker could use expired requests (prevented by deadline validation) |

### Security Guarantees

1. **No Parameter Swaps**: Changing any parameter changes the hash
2. **No Amount Tampering**: `total_amount` is included in hash
3. **No Account Misdirection**: All four account addresses are included
4. **No Token Confusion**: `usdc_contract` is included
5. **No Impersonation**: `from` address is included
6. **No Replays**: `nonce` is included

## Usage Examples

### Example 1: Basic Request Hashing

```rust
use soroban_sdk::{Address, Env};
use remittance_split::{RemittanceSplitClient, DistributeUsdcRequest, AccountGroup};

let env = Env::default();
let contract_id = /* ... */;
let client = RemittanceSplitClient::new(&env, &contract_id);

// Create request
let request = DistributeUsdcRequest {
    usdc_contract: Address::from_string(&env, "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAXJW3"),
    from: Address::from_string(&env, "GBXX..."),
    nonce: 5,
    accounts: AccountGroup {
        spending: Address::from_string(&env, "GCXX1..."),
        savings: Address::from_string(&env, "GCXX2..."),
        bills: Address::from_string(&env, "GCXX3..."),
        insurance: Address::from_string(&env, "GCXX4..."),
    },
    total_amount: 100_000_000, // 100 USDC (6 decimals)
    deadline: 1700000000 + 300, // 5 minutes from reference time
};

// Get request hash
let hash = client.get_request_hash(&request);
println!("Request hash: {:?}", hash); // 32-byte hash
```

### Example 2: Deadline Boundary Testing

```rust
// Get current timestamp
let current_time = env.ledger().timestamp();

// Valid deadline: 10 minutes from now
let valid_deadline = current_time + 600;

// Invalid deadline: More than 1 hour from now
let too_far_deadline = current_time + 3700; // Exceeds MAX_DEADLINE_WINDOW_SECS (3600)

// Invalid deadline: In the past
let past_deadline = current_time - 100;

// Invalid deadline: Exactly now (must be in future)
let now_deadline = current_time;
```

### Example 3: Full Distribution with Hash Verification

```rust
// Step 1: Create request
let request = DistributeUsdcRequest {
    usdc_contract,
    from: payer,
    nonce: 0,
    accounts,
    total_amount: 1_000_000,
    deadline: current_time + 300,
};

// Step 2: Get hash
let hash = client.get_request_hash(&request);

// Step 3: Off-chain signing (not shown here)
// signer_key.sign(hash) → signature

// Step 4: Submit distribution with verified hash
let result = client.distribute_usdc_with_hash_and_deadline(&request, &hash)?;
assert!(result);
```

## Validation Rules

### Deadline Validation

The contract enforces strict deadline rules:

1. **Deadline must be in the future**: `current_time < deadline`
2. **Deadline must not be zero**: `deadline != 0`
3. **Deadline must not exceed MAX_DEADLINE_WINDOW_SECS**: `deadline - current_time <= 3600`

```
current_time = 1000
MAX_DEADLINE_WINDOW_SECS = 3600

INVALID: deadline = 999 (in past)
INVALID: deadline = 0 (zero)
INVALID: deadline = 1000 (same as now)
INVALID: deadline = 5000 (1000 + 4000 > 3600)

VALID: deadline = 1001 (just in future)
VALID: deadline = 4600 (1000 + 3600 = exactly MAX_DEADLINE_WINDOW_SECS)
```

### Request Hash Verification

The contract verifies: `compute_request_hash(request) == provided_hash`

If hashes don't match, the transaction fails with `RequestHashMismatch` error.

## Security Best Practices

### For Integrators

1. **Verify Parameter Accuracy**: Before calling `get_request_hash()`, verify all parameters are correct
2. **Use Current Nonce**: Always use the latest nonce (call `get_nonce()` first)
3. **Set Reasonable Deadline**: Use 5-10 minute deadlines, not hours
4. **Validate Amounts**: Ensure `total_amount` is the intended amount
5. **Verify Accounts**: Double-check all four destination addresses
6. **Handle Hash Changes**: If any parameter changes, must request new hash

### For Signers

1. **Inspect Request Before Signing**: Verify all parameters match intent
2. **Verify Deadline is Reasonable**: Reject requests with too-far deadlines
3. **Check Recipient Addresses**: Ensure destination accounts are correct
4. **Verify Amount**: Confirm the `total_amount` is expected
5. **Sign Deterministically**: Use same hash every time for same request
6. **Reject Replayed Requests**: After signing, don't sign again with same nonce

## Error Handling

### Possible Errors

```rust
pub enum RemittanceSplitError {
    InvalidAmount,          // total_amount <= 0
    DeadlineExpired,        // current_time > deadline
    InvalidDeadline,        // deadline == 0 or deadline too far in future
    InvalidNonce,           // nonce doesn't match expected value
    RequestHashMismatch,    // provided hash != computed hash
    Overflow,               // Arithmetic overflow in split calculation
    // ... other errors
}
```

### Error Recovery

```rust
match client.try_distribute_usdc_with_hash_and_deadline(&request, &hash) {
    Ok(true) => println!("Distribution succeeded"),
    Err(Ok(RemittanceSplitError::DeadlineExpired)) => {
        println!("Request deadline has passed - create new request");
        // Get new deadline and rehash
    }
    Err(Ok(RemittanceSplitError::RequestHashMismatch)) => {
        println!("Hash mismatch - request parameters were tampered");
        // Verify parameters and get new hash
    }
    Err(Ok(RemittanceSplitError::InvalidNonce)) => {
        println!("Nonce mismatch - transaction already executed");
        // Get updated nonce
    }
    Err(err) => println!("Other error: {:?}", err),
}
```

## Hash Determinism Guarantees

The request hash is guaranteed to be deterministic across:

- Multiple calls with same request
- Different contract versions (domain separator prevents cross-version)
- Off-chain computation and on-chain verification

```rust
// These always produce identical hashes
let hash1 = client.get_request_hash(&request);
let hash2 = client.get_request_hash(&request);
let hash3 = client.compute_request_hash(&env, &request);

assert_eq!(hash1, hash2);
assert_eq!(hash2, hash3);
```

## Test Vectors

The contract includes comprehensive test vectors covering:

1. **Hash Determinism**: Same inputs produce same hash
2. **Parameter Sensitivity**: Changing any parameter changes hash
3. **Deadline Validation**: Boundary cases for deadline windows
4. **Hash Mismatch Detection**: Wrong hash is rejected
5. **Cross-call Consistency**: Hash consistent across calls

Run tests with:
```bash
cargo test -p remittance_split request_hash
```

## Compatibility Notes

- **Soroban SDK**: 21.0.0+
- **Protocol Version**: 20+ (Soroban Phase 1)
- **Domain Separator**: "distribute_usdc_v1" (prevents cross-version attacks)

### Future Versions

If a new version introduces incompatible changes, a new domain separator will be used:
- Version 1: "distribute_usdc_v1"
- Version 2 (hypothetical): "distribute_usdc_v2"

This ensures old signatures cannot be used with new contracts.

## FAQ

### Q: Can I change the deadline after getting the hash?

**A:** No. Changing the deadline changes the hash. You must get a new hash and have it signed again.

### Q: What if the deadline passes before the transaction is submitted?

**A:** The contract will reject it with `DeadlineExpired`. Create a new request with a future deadline.

### Q: Can I reuse a hash for multiple transactions?

**A:** No. Each transaction must have a unique nonce, which changes the hash. This prevents replay attacks.

### Q: How do I verify the hash off-chain?

**A:** Implement the same XDR serialization + SHA-256 computation. See the contract's `compute_request_hash()` implementation.

### Q: What happens if amounts don't split evenly?

**A:** The contract allocates remainders to the insurance category. This is deterministic and included in the hash.

### Q: Can the deadline be more than 1 hour in the future?

**A:** No. The contract enforces MAX_DEADLINE_WINDOW_SECS = 3600 seconds. Deadlines too far in the future are rejected.

## Troubleshooting

### Hash Mismatch Issues

```
Error: RequestHashMismatch
```

Possible causes:
1. Request parameters were modified
2. Hash was corrupted in transmission
3. Different Soroban version used
4. Incorrect domain separator

**Solution**: Verify all parameters match original request and recompute hash.

### Deadline Expired Issues

```
Error: DeadlineExpired
```

Possible causes:
1. Transaction was submitted after deadline passed
2. Ledger time was updated between hash creation and submission
3. Deadline was set too short

**Solution**: Create new request with fresh deadline.

### Invalid Nonce Issues

```
Error: InvalidNonce
```

Possible causes:
1. Nonce value doesn't match current contract state
2. Transaction was already executed
3. Nonce was incremented by another transaction

**Solution**: Call `get_nonce()` to get current nonce and create new request.

## References

- [SECURITY_REVIEW_SUMMARY.md](../SECURITY_REVIEW_SUMMARY.md) - Security considerations
- [THREAT_MODEL.md](../THREAT_MODEL.md) - Threat analysis including signing scenarios
- [remittance_split/src/lib.rs](src/lib.rs) - Contract implementation
- [remittance_split/src/test.rs](src/test.rs) - Test vectors and examples

## Support

For issues or questions:
1. Review test vectors in `src/test.rs`
2. Check security documentation in main repo
3. Review contract source code comments
4. File issue on GitHub with reproduction case
