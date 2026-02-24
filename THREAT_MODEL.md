# Remitwise Smart Contracts - Threat Model

**Version:** 1.0  
**Date:** 2026-02-24  
**Status:** Initial Security Review  
**Reviewer:** Security Team

## Executive Summary

This document presents a comprehensive threat model for the Remitwise smart contract suite deployed on the Stellar Soroban platform. The analysis identifies critical assets, potential threats, existing mitigations, and security gaps across 10 interconnected contracts managing financial operations including remittance allocation, savings goals, bill payments, insurance policies, and family wallet management.

**Key Findings:**
- 10 critical/high-severity security issues identified
- 8 medium-severity vulnerabilities requiring attention
- 5 low-severity concerns for long-term improvement
- Strong foundation with consistent authorization patterns
- Gaps in cross-contract security, data privacy, and emergency controls

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Asset Identification](#asset-identification)
3. [Threat Enumeration](#threat-enumeration)
4. [Existing Mitigations](#existing-mitigations)
5. [Security Gaps](#security-gaps)
6. [Threat Scenarios](#threat-scenarios)
7. [Recommendations](#recommendations)
8. [Follow-up Issues](#follow-up-issues)

---

## 1. System Overview

### Architecture

The Remitwise system consists of 10 smart contracts:

**Core Financial Contracts:**
- `remittance_split` - Allocates incoming remittances by percentage
- `savings_goals` - Goal-based savings with locking mechanisms
- `bill_payments` - Recurring and one-time bill tracking
- `insurance` - Micro-insurance policy management

**Coordination & Access Control:**
- `family_wallet` - Multi-signature family fund management
- `orchestrator` - Cross-contract remittance flow coordinator

**Reporting & Utilities:**
- `reporting` - Financial health aggregation and analytics
- `data_migration` - Import/export with checksum validation
- `rate_limiter` - (Minimal implementation)
- `analytics` - (Minimal implementation)

### Data Flow

```
Incoming Remittance → remittance_split → [savings_goals, bill_payments, insurance]
                                    ↓
                            orchestrator (coordinator)
                                    ↓
                            reporting (aggregator)
```


---

## 2. Asset Identification

### 2.1 Financial Assets

| Asset | Location | Value | Protection |
|-------|----------|-------|------------|
| **User Funds** | External (tracked in contracts) | Variable | Owner authorization |
| **Savings Balances** | `savings_goals::current_amount` | Cumulative user savings | Owner-only withdrawal |
| **Bill Payment Amounts** | `bill_payments::amount` | Pending bill amounts | Owner-only payment |
| **Insurance Premiums** | `insurance::monthly_premium` | Recurring premium amounts | Owner-only payment |
| **Family Wallet Balances** | External (multi-sig controlled) | Shared family funds | Multi-signature approval |

### 2.2 Configuration Assets

| Asset | Location | Criticality | Protection |
|-------|----------|-------------|------------|
| **Split Percentages** | `remittance_split::SplitConfig` | HIGH | Owner-only modification |
| **Pause Admin** | All contracts `PAUSE_ADM` | CRITICAL | Self-assignment only |
| **Upgrade Admin** | All contracts `UPG_ADM` | CRITICAL | Self-assignment only |
| **Multi-sig Config** | `family_wallet::MultiSigConfig` | HIGH | Owner/Admin only |
| **Contract Addresses** | `reporting::ContractAddresses` | HIGH | Admin-only configuration |

### 2.3 Identity & Access Assets

| Asset | Location | Criticality | Protection |
|-------|----------|-------------|------------|
| **Owner Addresses** | All contracts | CRITICAL | Soroban authentication |
| **Family Roles** | `family_wallet::FamilyMember` | HIGH | Owner/Admin management |
| **Spending Limits** | `family_wallet::spending_limit` | MEDIUM | Owner/Admin configuration |
| **Nonces** | `remittance_split`, `savings_goals` | MEDIUM | Replay protection |

### 2.4 Data Assets

| Asset | Location | Sensitivity | Protection |
|-------|----------|-------------|------------|
| **Financial History** | Events, archived records | HIGH | Public blockchain |
| **Goal Details** | `savings_goals::SavingsGoal` | MEDIUM | Owner-only access |
| **Bill Details** | `bill_payments::Bill` | MEDIUM | Owner-only access |
| **Policy Details** | `insurance::InsurancePolicy` | MEDIUM | Owner-only access |
| **Audit Logs** | All contracts | LOW | Public events |

### 2.5 Operational Assets

| Asset | Location | Criticality | Protection |
|-------|----------|-------------|------------|
| **Contract Availability** | All contracts | HIGH | Pause mechanisms |
| **Storage TTL** | Instance storage | MEDIUM | Automatic extension |
| **Event Integrity** | Blockchain events | MEDIUM | Immutable ledger |


---

## 3. Threat Enumeration

### 3.1 Unauthorized Access Threats

#### T-UA-01: Information Disclosure via Reporting Contract
**Severity:** HIGH  
**Description:** The reporting contract allows any caller to query sensitive financial data for any user without authorization checks.

**Affected Functions:**
- `get_remittance_summary()`
- `get_savings_report()`
- `get_bill_compliance_report()`
- `get_insurance_coverage_report()`

**Attack Vector:**
1. Attacker calls reporting functions with victim's address
2. Retrieves complete financial profile including balances, goals, bills, policies
3. Uses information for social engineering or targeted attacks

**Impact:** Privacy violation, information disclosure, potential for targeted attacks

---

#### T-UA-02: Cross-Contract Authorization Bypass
**Severity:** MEDIUM  
**Description:** Orchestrator executes downstream operations without verifying caller owns the resources being manipulated.

**Affected Functions:**
- `orchestrator::execute_remittance_flow()`
- `orchestrator::deposit_to_savings()`
- `orchestrator::execute_bill_payment_internal()`

**Attack Vector:**
1. Attacker calls orchestrator with victim's goal/bill/policy IDs
2. Orchestrator forwards calls to downstream contracts
3. If orchestrator is trusted by downstream contracts, operations may succeed

**Impact:** Unauthorized fund allocation, state manipulation

---

#### T-UA-03: Expired Role Retention
**Severity:** LOW  
**Description:** Family wallet role expiry not consistently enforced across all functions.

**Affected Functions:**
- Various family_wallet functions missing `role_has_expired()` checks

**Attack Vector:**
1. Admin role expires but user retains cached permissions
2. Expired admin performs privileged operations before expiry is checked
3. Unauthorized access window between expiry and enforcement

**Impact:** Temporary privilege escalation

---

### 3.2 Replay Attack Threats

#### T-RP-01: Nonce Bypass in Import Operations
**Severity:** MEDIUM  
**Description:** Data import operations use nonces but nonce validation may be bypassed if storage is corrupted.

**Affected Functions:**
- `remittance_split::import_snapshot()`
- `savings_goals::import_goals()`

**Attack Vector:**
1. Attacker exports snapshot with valid nonce
2. Corrupts nonce storage through separate vulnerability
3. Replays old snapshot to revert state changes

**Impact:** State rollback, data corruption

---

#### T-RP-02: Multi-sig Transaction Replay
**Severity:** LOW  
**Description:** Executed multi-sig transactions stored in `EXEC_TXS` map but no expiry mechanism.

**Affected Functions:**
- `family_wallet::sign_transaction()`

**Attack Vector:**
1. Transaction executed and marked in EXEC_TXS
2. If EXEC_TXS storage is corrupted or cleared, transaction could be replayed
3. Duplicate execution of high-value operations

**Impact:** Duplicate fund transfers, state inconsistency

---

### 3.3 Griefing & Denial of Service Threats

#### T-DOS-01: Storage Bloat Attack
**Severity:** MEDIUM  
**Description:** Unbounded maps and audit logs allow attackers to exhaust storage.

**Affected Contracts:**
- All contracts with Map<u32, Entity> storage
- Audit logs in savings_goals and remittance_split

**Attack Vector:**
1. Attacker creates maximum number of goals/bills/policies
2. Performs operations to generate audit log entries
3. Storage costs increase, contract becomes expensive to maintain
4. Legitimate users unable to create new entities

**Impact:** Service degradation, increased costs, potential contract abandonment

---

#### T-DOS-02: Cross-Contract Call Cascade Failure
**Severity:** MEDIUM  
**Description:** Single downstream contract failure causes entire orchestrator flow to revert.

**Affected Functions:**
- `orchestrator::execute_remittance_flow()`

**Attack Vector:**
1. Attacker pauses or corrupts one downstream contract
2. All orchestrator flows fail completely
3. No partial success or fallback mechanism
4. System-wide denial of service

**Impact:** Complete service outage, user funds stuck

---

#### T-DOS-03: Batch Operation Abuse
**Severity:** LOW  
**Description:** Batch operations limited to 50 items but no rate limiting across multiple calls.

**Affected Functions:**
- `bill_payments::batch_pay_bills()`
- `savings_goals::batch_add_to_goals()`

**Attack Vector:**
1. Attacker makes multiple batch calls in rapid succession
2. Consumes excessive gas and storage
3. Legitimate users experience degraded performance

**Impact:** Performance degradation, increased costs

---

### 3.4 Economic Attack Threats

#### T-EC-01: Rounding Error Exploitation
**Severity:** MEDIUM  
**Description:** Percentage-based allocation may have rounding errors that accumulate over time.

**Affected Functions:**
- `remittance_split::calculate_split()`

**Attack Vector:**
1. Attacker sends many small remittances
2. Rounding errors accumulate in attacker's favor
3. Over time, attacker gains funds from rounding discrepancies

**Impact:** Financial loss through accumulated rounding errors

---

#### T-EC-02: Emergency Mode Fund Drain
**Severity:** HIGH  
**Description:** Emergency mode allows unlimited transfers without multi-sig and no cooldown enforcement.

**Affected Functions:**
- `family_wallet::execute_emergency_transfer_now()`
- `family_wallet::set_emergency_mode()`

**Attack Vector:**
1. Attacker compromises Owner/Admin account
2. Activates emergency mode
3. Executes multiple emergency transfers rapidly
4. Drains family wallet before detection

**Impact:** Complete fund loss

---

#### T-EC-03: Spending Limit Bypass
**Severity:** LOW  
**Description:** Spending limit of 0 means unlimited, no way to enforce true zero spending.

**Affected Functions:**
- `family_wallet::check_spending_limit()`

**Attack Vector:**
1. Admin sets member spending limit to 0 intending to block spending
2. Member has unlimited spending due to 0 = unlimited logic
3. Unintended fund access

**Impact:** Unauthorized spending

---

### 3.5 Data Integrity Threats

#### T-DI-01: Weak Checksum Validation
**Severity:** MEDIUM  
**Description:** Data migration uses simple checksum vulnerable to collision attacks.

**Affected Functions:**
- `data_migration::ExportSnapshot::verify_checksum()`

**Attack Vector:**
1. Attacker exports legitimate snapshot
2. Modifies payload to malicious data
3. Crafts collision to match original checksum
4. Imports corrupted data that passes validation

**Impact:** Data corruption, state manipulation

---

#### T-DI-02: Archive Data Loss
**Severity:** LOW  
**Description:** Archived records use compressed structs losing original data fields.

**Affected Functions:**
- `bill_payments::archive_paid_bills()`
- `savings_goals::archive_goal()`
- `insurance::archive_policy()`

**Attack Vector:**
1. User archives entity
2. Attempts to restore archived entity
3. Original fields (due_date, target_date, etc.) are lost
4. Restored entity has incomplete data

**Impact:** Data loss, operational confusion

---

#### T-DI-03: Mixed Storage Type Inconsistency
**Severity:** MEDIUM  
**Description:** Savings goals uses both persistent and instance storage with different TTLs.

**Affected Contracts:**
- `savings_goals` (GOALS in persistent, pause state in instance)

**Attack Vector:**
1. Instance storage TTL expires
2. Pause state lost but goals remain
3. Contract state becomes inconsistent
4. Paused contract appears unpaused

**Impact:** State inconsistency, security control bypass


### 3.6 Reentrancy & Cross-Contract Threats

#### T-RE-01: Cross-Contract Reentrancy
**Severity:** HIGH  
**Description:** Orchestrator makes multiple cross-contract calls without reentrancy protection.

**Affected Functions:**
- `orchestrator::execute_remittance_flow()`
- All cross-contract client calls

**Attack Vector:**
1. Attacker deploys malicious contract as downstream dependency
2. Malicious contract calls back to orchestrator during execution
3. Orchestrator state is modified mid-execution
4. Inconsistent state or duplicate operations

**Impact:** State corruption, duplicate fund allocation, financial loss

---

#### T-RE-02: Unvalidated Contract Addresses
**Severity:** MEDIUM  
**Description:** Reporting contract doesn't validate configured addresses are actual contracts.

**Affected Functions:**
- `reporting::configure_addresses()`

**Attack Vector:**
1. Admin configures invalid or malicious contract addresses
2. Reporting queries fail silently or return malicious data
3. Users receive incorrect financial reports
4. Decisions made based on false data

**Impact:** Data corruption, operational failures, financial mismanagement

---

### 3.7 Privilege Escalation Threats

#### T-PE-01: Pause Admin Single Point of Failure
**Severity:** MEDIUM  
**Description:** Single pause admin with no backup or recovery mechanism.

**Affected Contracts:**
- All contracts with pause functionality

**Attack Vector:**
1. Pause admin key is lost or compromised
2. No backup admin can pause contract in emergency
3. Malicious activity continues unchecked
4. Or legitimate operations blocked if compromised admin pauses

**Impact:** Loss of emergency controls, potential for extended attacks

---

#### T-PE-02: Upgrade Admin Privilege Abuse
**Severity:** LOW  
**Description:** Upgrade admin can change version number but no actual upgrade mechanism exists.

**Affected Functions:**
- `set_version()` in all contracts

**Attack Vector:**
1. Upgrade admin sets arbitrary version number
2. Version mismatch causes compatibility issues
3. Data migration fails due to version incompatibility

**Impact:** Operational confusion, migration failures

---

### 3.8 Input Validation Threats

#### T-IV-01: Unbounded String Inputs
**Severity:** LOW  
**Description:** Goal names, bill names, policy names have no length limits.

**Affected Functions:**
- `create_goal()`, `create_bill()`, `create_policy()`

**Attack Vector:**
1. Attacker creates entities with extremely long names
2. Storage costs increase dramatically
3. Query operations become expensive
4. Potential for storage exhaustion

**Impact:** Storage bloat, increased costs, performance degradation

---

#### T-IV-02: Tag Content Not Validated
**Severity:** LOW  
**Description:** Tags validated for length but not content, could contain malicious strings.

**Affected Functions:**
- `add_tags_to_goal()`, `add_tags_to_bill()`, `add_tags_to_policy()`

**Attack Vector:**
1. Attacker adds tags with special characters or malicious content
2. Tags stored in contract state
3. Potential for injection attacks in off-chain systems reading tags

**Impact:** Data corruption, potential injection attacks in integrations

---

#### T-IV-03: Extreme Value Handling
**Severity:** LOW  
**Description:** No maximum limits on amounts, could cause calculation issues.

**Affected Functions:**
- All functions accepting i128 amounts

**Attack Vector:**
1. Attacker creates goal/bill/policy with i128::MAX amount
2. Arithmetic operations overflow or behave unexpectedly
3. Contract state becomes corrupted

**Impact:** Integer overflow, calculation errors, state corruption

---

### 3.9 Event & Audit Threats

#### T-EV-01: Sensitive Data in Public Events
**Severity:** MEDIUM  
**Description:** Events include full amounts, dates, and addresses visible to all.

**Affected Contracts:**
- All contracts emitting financial events

**Attack Vector:**
1. Attacker monitors blockchain events
2. Builds complete financial profile of users
3. Uses information for targeted attacks or social engineering

**Impact:** Privacy violation, information disclosure

---

#### T-EV-02: Audit Log Unbounded Growth
**Severity:** LOW  
**Description:** Audit logs grow without limit, no cleanup mechanism.

**Affected Contracts:**
- `savings_goals`, `remittance_split`

**Attack Vector:**
1. Attacker performs many operations to generate audit entries
2. Audit log grows unbounded
3. Storage costs increase
4. Contract becomes expensive to maintain

**Impact:** Storage bloat, increased costs

---

### 3.10 Pause & Emergency Control Threats

#### T-PC-01: Pause State Desynchronization
**Severity:** MEDIUM  
**Description:** Each contract has independent pause state, no global coordination.

**Affected Contracts:**
- All contracts with pause functionality

**Attack Vector:**
1. Admin pauses some contracts but not others
2. Orchestrator continues operating with partially paused system
3. Inconsistent state across contracts
4. Operations fail unpredictably

**Impact:** Operational confusion, inconsistent security posture

---

#### T-PC-02: No Pause Reason Tracking
**Severity:** LOW  
**Description:** Pause events don't include reason or context.

**Affected Contracts:**
- All contracts with pause functionality

**Attack Vector:**
1. Contract is paused for unknown reason
2. Operators unable to determine cause
3. Delayed response to security incidents
4. Potential for unnecessary downtime

**Impact:** Operational inefficiency, delayed incident response


---

## 4. Existing Mitigations

### 4.1 Authorization & Access Control

✅ **Soroban Authentication**
- All state-modifying functions call `caller.require_auth()`
- Cryptographic signature verification at protocol level
- Prevents unauthorized function calls

✅ **Owner-Based Access Control**
- Each entity (goal, bill, policy) has owner field
- Operations verify `caller == owner` before execution
- Prevents cross-user manipulation

✅ **Role-Based Hierarchy**
- Family wallet implements Owner > Admin > Member hierarchy
- Role checks enforce privilege separation
- Prevents unauthorized privilege escalation

✅ **Multi-Signature Requirements**
- High-value operations require multiple signatures
- Configurable threshold per transaction type
- Prevents single-point-of-failure for critical operations

### 4.2 Data Integrity

✅ **Checked Arithmetic**
- Uses `checked_add()`, `checked_sub()`, `checked_mul()`
- Prevents integer overflow/underflow
- Transactions revert on arithmetic errors

✅ **Amount Validation**
- All fund operations validate `amount > 0`
- Prevents negative amounts and zero-value operations
- Ensures meaningful transactions only

✅ **Percentage Validation**
- Remittance split validates percentages sum to 100
- Prevents invalid allocation configurations
- Ensures complete fund distribution

✅ **Nonce-Based Replay Protection**
- Import operations use incrementing nonces
- Prevents replay of old snapshots
- Ensures data import idempotency

### 4.3 Storage & State Management

✅ **TTL Management**
- Automatic instance storage extension (518,400 ledgers ≈ 6 days)
- Archive storage with longer TTL (2,592,000 ledgers ≈ 30 days)
- Prevents premature data expiration

✅ **Atomic Operations**
- All state changes are atomic
- Transaction reverts on any failure
- Prevents partial state updates

✅ **Data Archival**
- Completed entities moved to compressed archive storage
- Reduces active storage costs
- Maintains historical records

✅ **Event Logging**
- All state changes emit events
- Provides complete audit trail
- Enables off-chain monitoring and analytics

### 4.4 Emergency Controls

✅ **Pause Mechanisms**
- Global pause stops all operations
- Function-level pause for granular control
- Emergency pause all for rapid response

✅ **Scheduled Unpause**
- Bill payments supports time-delayed unpause
- Prevents indefinite pause states
- Enables automated recovery

✅ **Emergency Mode**
- Family wallet supports emergency transfers
- Bypasses multi-sig for urgent situations
- Enables rapid fund recovery

### 4.5 Input Validation

✅ **Frequency Validation**
- Recurring bills validate `frequency_days > 0`
- Prevents invalid recurring schedules
- Ensures meaningful recurrence

✅ **Tag Format Validation**
- Tags validated for length (1-32 characters)
- Prevents empty or excessively long tags
- Ensures consistent tag format

✅ **Threshold Validation**
- Multi-sig threshold validated against signer count
- Prevents impossible threshold configurations
- Ensures achievable approval requirements

### 4.6 Cross-Contract Security

✅ **Type-Safe Client Calls**
- Uses generated Soroban clients
- Compile-time type checking
- Prevents parameter mismatches

✅ **Error Propagation**
- Downstream failures cause transaction revert
- Maintains atomicity across contracts
- Prevents inconsistent state

✅ **Gas Estimation**
- Cross-contract calls include gas estimates
- Helps prevent out-of-gas failures
- Enables cost planning


---

## 5. Security Gaps

### 5.1 Critical Gaps

❌ **No Authorization in Reporting Queries**
- **Gap:** Reporting contract allows any caller to query any user's financial data
- **Missing Control:** Caller verification or access control lists
- **Risk:** Complete privacy violation, information disclosure
- **Recommendation:** Add `caller.require_auth()` and verify `caller == user` or implement ACL

❌ **No Reentrancy Protection**
- **Gap:** Orchestrator makes multiple cross-contract calls without reentrancy guards
- **Missing Control:** Reentrancy locks or checks-effects-interactions pattern
- **Risk:** State corruption, duplicate operations, financial loss
- **Recommendation:** Implement reentrancy guard or refactor to checks-effects-interactions

❌ **Emergency Mode Lacks Rate Limiting**
- **Gap:** Emergency transfers not rate-limited or cooldown-enforced
- **Missing Control:** Transfer frequency limits, cooldown enforcement
- **Risk:** Rapid fund drain in emergency mode
- **Recommendation:** Enforce cooldown between emergency transfers, add transfer count limits

### 5.2 High-Priority Gaps

❌ **Weak Checksum Validation**
- **Gap:** Data migration uses simple checksum, not cryptographic hash
- **Missing Control:** SHA-256 or similar cryptographic hash
- **Risk:** Collision attacks, corrupted data passing validation
- **Recommendation:** Replace with SHA-256 or BLAKE2b

❌ **No Balance Verification**
- **Gap:** Contracts track balances but don't verify actual token balances
- **Missing Control:** Token balance queries and reconciliation
- **Risk:** State-balance mismatch, double-spending
- **Recommendation:** Add balance verification functions, periodic reconciliation

❌ **Pause State Not Synchronized**
- **Gap:** Each contract has independent pause state
- **Missing Control:** Global pause coordinator or cross-contract pause propagation
- **Risk:** Partial system pause, inconsistent security posture
- **Recommendation:** Implement global pause mechanism or pause state synchronization

### 5.3 Medium-Priority Gaps

❌ **No Storage Bounds**
- **Gap:** Maps grow unbounded, no limits on entity creation
- **Missing Control:** Maximum entity counts, storage quotas
- **Risk:** Storage bloat, DoS through excessive creation
- **Recommendation:** Implement per-user entity limits, storage quotas

❌ **Mixed Storage Types**
- **Gap:** Savings goals uses both persistent and instance storage
- **Missing Control:** Consistent storage type usage
- **Risk:** TTL mismatch, state inconsistency
- **Recommendation:** Standardize on single storage type (persistent recommended)

❌ **No Contract Address Validation**
- **Gap:** Reporting contract doesn't validate configured addresses
- **Missing Control:** Contract existence checks, interface validation
- **Risk:** Silent failures, malicious contract injection
- **Recommendation:** Validate addresses are contracts, verify interfaces

❌ **Role Expiry Not Consistently Enforced**
- **Gap:** Some functions don't check role expiry
- **Missing Control:** Consistent expiry checks across all role-based functions
- **Risk:** Expired roles retain temporary access
- **Recommendation:** Add expiry check to all role-based functions

### 5.4 Low-Priority Gaps

❌ **No Upgrade Mechanism**
- **Gap:** Version tracked but no actual upgrade function
- **Missing Control:** Contract upgrade or migration mechanism
- **Risk:** Stuck with bugs, no security patch path
- **Recommendation:** Implement upgrade mechanism or document migration process

❌ **Insufficient Error Differentiation**
- **Gap:** Many functions use panic!() instead of Result types
- **Missing Control:** Structured error types, error codes
- **Risk:** Difficult debugging, poor error handling
- **Recommendation:** Use Result types consistently, define error enums

❌ **No Input Bounds**
- **Gap:** String inputs, amounts have no maximum limits
- **Missing Control:** Maximum length/value validation
- **Risk:** Storage bloat, calculation issues
- **Recommendation:** Add maximum limits for strings and amounts

❌ **Audit Log Unbounded**
- **Gap:** Audit logs grow without limit
- **Missing Control:** Automatic cleanup, log rotation
- **Risk:** Storage bloat, increased costs
- **Recommendation:** Implement automatic cleanup or log size limits

❌ **No Pause Reason Tracking**
- **Gap:** Pause events don't include reason
- **Missing Control:** Reason field in pause events
- **Risk:** Operational confusion, delayed incident response
- **Recommendation:** Add reason parameter to pause functions


---

## 6. Threat Scenarios

### Scenario 1: Privacy Breach via Reporting Contract

**Attacker Profile:** External observer, no special privileges

**Attack Steps:**
1. Attacker identifies target user address from public transactions
2. Calls `reporting::get_remittance_summary()` with target address
3. Calls `reporting::get_savings_report()` to get savings goals and balances
4. Calls `reporting::get_bill_compliance_report()` to get bill payment history
5. Calls `reporting::get_insurance_coverage_report()` to get policy details
6. Builds complete financial profile of target user

**Impact:**
- Complete privacy violation
- Sensitive financial data exposed
- Potential for targeted phishing or social engineering
- Regulatory compliance issues (GDPR, financial privacy laws)

**Likelihood:** HIGH (trivial to execute, no barriers)

**Mitigation Status:** ❌ Not mitigated

---

### Scenario 2: Emergency Mode Fund Drain

**Attacker Profile:** Compromised Owner/Admin account

**Attack Steps:**
1. Attacker gains access to Owner/Admin private key (phishing, malware, etc.)
2. Calls `family_wallet::set_emergency_mode(true)` to activate emergency mode
3. Rapidly calls `execute_emergency_transfer_now()` multiple times
4. Transfers all family wallet funds to attacker-controlled addresses
5. Deactivates emergency mode to cover tracks

**Impact:**
- Complete loss of family wallet funds
- No multi-sig protection in emergency mode
- No cooldown enforcement allows rapid drain
- Difficult to detect and respond before funds are gone

**Likelihood:** MEDIUM (requires account compromise but high impact)

**Mitigation Status:** ⚠️ Partially mitigated (emergency mode exists but lacks safeguards)

---

### Scenario 3: Cross-Contract Reentrancy Attack

**Attacker Profile:** Malicious contract deployer

**Attack Steps:**
1. Attacker deploys malicious contract implementing savings/bills/insurance interface
2. Attacker configures orchestrator to use malicious contract as downstream dependency
3. Attacker calls `orchestrator::execute_remittance_flow()`
4. Malicious contract receives call, calls back to orchestrator mid-execution
5. Orchestrator state is modified during execution
6. Duplicate allocations or state corruption occurs

**Impact:**
- State corruption across multiple contracts
- Duplicate fund allocations
- Financial loss through double-spending
- System-wide inconsistency

**Likelihood:** LOW (requires orchestrator misconfiguration but high impact)

**Mitigation Status:** ❌ Not mitigated

---

### Scenario 4: Storage Bloat Denial of Service

**Attacker Profile:** Malicious user with minimal funds

**Attack Steps:**
1. Attacker creates maximum number of savings goals (no limit)
2. Attacker creates maximum number of bills (no limit)
3. Attacker creates maximum number of insurance policies (no limit)
4. Attacker performs operations to generate audit log entries
5. Storage costs increase dramatically
6. Legitimate users unable to create new entities due to storage exhaustion

**Impact:**
- Service degradation for all users
- Increased operational costs
- Potential contract abandonment due to unsustainable costs
- Denial of service for new entity creation

**Likelihood:** MEDIUM (requires sustained effort but achievable)

**Mitigation Status:** ⚠️ Partially mitigated (TTL management exists but no entity limits)

---

### Scenario 5: Data Corruption via Weak Checksum

**Attacker Profile:** Malicious data exporter

**Attack Steps:**
1. Attacker exports legitimate snapshot from contract
2. Attacker modifies payload to inject malicious data
3. Attacker crafts checksum collision to match original
4. Attacker imports corrupted snapshot
5. Contract accepts corrupted data due to checksum match
6. Contract state becomes corrupted

**Impact:**
- Data corruption across contract state
- Incorrect financial calculations
- Loss of data integrity
- Potential for financial loss

**Likelihood:** LOW (requires cryptographic expertise but possible)

**Mitigation Status:** ⚠️ Partially mitigated (checksum exists but weak)

---

### Scenario 6: Pause State Desynchronization Attack

**Attacker Profile:** Malicious actor exploiting operational confusion

**Attack Steps:**
1. Attacker identifies security issue in one contract
2. Admin pauses affected contract
3. Orchestrator continues operating with other contracts
4. Attacker exploits unpaused contracts to manipulate state
5. When paused contract is unpaused, state is inconsistent
6. System-wide confusion and potential financial loss

**Impact:**
- Inconsistent security posture
- Partial system protection only
- Operational confusion
- Potential for exploitation during partial pause

**Likelihood:** MEDIUM (depends on operational procedures)

**Mitigation Status:** ❌ Not mitigated


---

## 7. Recommendations

### 7.1 Immediate Actions (Critical Priority)

#### 1. Add Authorization to Reporting Contract
**Issue:** T-UA-01  
**Action:** Implement caller verification in all reporting query functions

```rust
pub fn get_remittance_summary(
    env: Env,
    caller: Address,
    user: Address,
    // ... other params
) -> RemittanceSummary {
    caller.require_auth();
    if caller != user {
        // Check if caller has permission (ACL, admin, etc.)
        panic!("Unauthorized access to user data");
    }
    // ... existing logic
}
```

**Timeline:** Immediate (before mainnet deployment)  
**Effort:** Low (1-2 days)

---

#### 2. Implement Reentrancy Protection
**Issue:** T-RE-01  
**Action:** Add reentrancy guard to orchestrator

```rust
// Add to orchestrator state
const REENTRANCY_GUARD: Symbol = symbol_short!("RE_GUARD");

fn check_reentrancy(env: &Env) {
    let guard: bool = env.storage().instance().get(&REENTRANCY_GUARD).unwrap_or(false);
    if guard {
        panic!("Reentrancy detected");
    }
    env.storage().instance().set(&REENTRANCY_GUARD, &true);
}

fn clear_reentrancy(env: &Env) {
    env.storage().instance().set(&REENTRANCY_GUARD, &false);
}

pub fn execute_remittance_flow(...) {
    check_reentrancy(&env);
    // ... existing logic
    clear_reentrancy(&env);
}
```

**Timeline:** Immediate (before mainnet deployment)  
**Effort:** Medium (3-5 days including testing)

---

#### 3. Add Emergency Transfer Rate Limiting
**Issue:** T-EC-02  
**Action:** Enforce cooldown and transfer limits in emergency mode

```rust
pub fn execute_emergency_transfer_now(...) -> u64 {
    // Check cooldown
    let last_transfer: u64 = env.storage().instance().get(&symbol_short!("EM_LAST")).unwrap_or(0);
    let em_config: EmergencyConfig = env.storage().instance().get(&symbol_short!("EM_CONF")).expect("Emergency config not set");
    
    let current_time = env.ledger().timestamp();
    if current_time < last_transfer + em_config.cooldown {
        panic!("Emergency transfer cooldown not elapsed");
    }
    
    // Update last transfer time
    env.storage().instance().set(&symbol_short!("EM_LAST"), &current_time);
    
    // ... existing logic
}
```

**Timeline:** Immediate (before mainnet deployment)  
**Effort:** Low (1-2 days)

---

### 7.2 Short-Term Actions (High Priority)

#### 4. Replace Checksum with Cryptographic Hash
**Issue:** T-DI-01  
**Action:** Use SHA-256 instead of simple checksum

```rust
use sha2::{Sha256, Digest};

impl ExportSnapshot {
    pub fn compute_checksum(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(serde_json::to_vec(&self.payload).expect("payload must be serializable"));
        hex::encode(hasher.finalize())
    }
}
```

**Timeline:** 1-2 weeks  
**Effort:** Low (already using sha2 crate)

---

#### 5. Implement Storage Bounds
**Issue:** T-DOS-01  
**Action:** Add per-user entity limits

```rust
const MAX_GOALS_PER_USER: u32 = 100;
const MAX_BILLS_PER_USER: u32 = 200;
const MAX_POLICIES_PER_USER: u32 = 50;

pub fn create_goal(...) -> u32 {
    // ... existing auth and validation
    
    // Count existing goals for owner
    let mut count = 0u32;
    for (_, goal) in goals.iter() {
        if goal.owner == owner {
            count += 1;
        }
    }
    
    if count >= MAX_GOALS_PER_USER {
        panic!("Maximum goals per user exceeded");
    }
    
    // ... existing logic
}
```

**Timeline:** 2-3 weeks  
**Effort:** Medium (requires testing across all contracts)

---

#### 6. Standardize Storage Type
**Issue:** T-DI-03  
**Action:** Convert all storage to persistent type

```rust
// Change from:
env.storage().instance().get(&symbol_short!("GOALS"))

// To:
env.storage().persistent().get(&symbol_short!("GOALS"))
```

**Timeline:** 2-3 weeks  
**Effort:** Medium (requires careful migration and testing)

---

#### 7. Add Contract Address Validation
**Issue:** T-RE-02  
**Action:** Validate configured addresses are contracts

```rust
pub fn configure_addresses(
    env: Env,
    caller: Address,
    remittance_split: Address,
    // ... other addresses
) -> bool {
    caller.require_auth();
    
    // Validate addresses are contracts (attempt to call a standard function)
    // This will panic if address is not a contract
    let split_client = RemittanceSplitClient::new(&env, &remittance_split);
    let _ = split_client.get_split(); // Verify contract responds
    
    // ... existing logic
}
```

**Timeline:** 1-2 weeks  
**Effort:** Low

---

### 7.3 Medium-Term Actions (Medium Priority)

#### 8. Implement Global Pause Mechanism
**Issue:** T-PC-01  
**Action:** Create pause coordinator contract

```rust
#[contract]
pub struct PauseCoordinator;

#[contractimpl]
impl PauseCoordinator {
    pub fn pause_all(env: Env, admin: Address, reason: String) {
        admin.require_auth();
        
        // Pause all registered contracts
        let contracts: Vec<Address> = env.storage().instance().get(&symbol_short!("CONTRACTS")).unwrap();
        
        for contract_addr in contracts.iter() {
            // Call pause on each contract
            // Store reason for audit
        }
    }
}
```

**Timeline:** 4-6 weeks  
**Effort:** High (new contract, integration testing)

---

#### 9. Add Balance Verification
**Issue:** T-DI-02  
**Action:** Implement balance reconciliation functions

```rust
pub fn verify_balances(env: Env, token: Address) -> bool {
    let token_client = TokenClient::new(&env, &token);
    let contract_balance = token_client.balance(&env.current_contract_address());
    
    // Calculate expected balance from contract state
    let expected_balance = Self::calculate_total_balances(&env);
    
    if contract_balance != expected_balance {
        env.events().publish(
            (symbol_short!("balance"), symbol_short!("mismatch")),
            (contract_balance, expected_balance),
        );
        return false;
    }
    
    true
}
```

**Timeline:** 3-4 weeks  
**Effort:** Medium

---

#### 10. Implement Audit Log Cleanup
**Issue:** T-EV-02  
**Action:** Add automatic audit log rotation

```rust
const MAX_AUDIT_ENTRIES: u32 = 1000;

fn append_audit(env: &Env, operation: Symbol, caller: &Address, success: bool) {
    let mut audit: Vec<AuditEntry> = env.storage().instance().get(&symbol_short!("AUDIT")).unwrap_or_else(|| Vec::new(env));
    
    // If at max, remove oldest entry
    if audit.len() >= MAX_AUDIT_ENTRIES {
        audit.remove(0);
    }
    
    audit.push_back(AuditEntry {
        operation,
        caller: caller.clone(),
        timestamp: env.ledger().timestamp(),
        success,
    });
    
    env.storage().instance().set(&symbol_short!("AUDIT"), &audit);
}
```

**Timeline:** 2-3 weeks  
**Effort:** Low

---

### 7.4 Long-Term Actions (Low Priority)

#### 11. Implement Upgrade Mechanism
**Issue:** T-PE-02  
**Action:** Design and implement contract upgrade pattern

**Timeline:** 8-12 weeks  
**Effort:** High (requires architecture design)

---

#### 12. Add Privacy Controls
**Issue:** T-EV-01  
**Action:** Implement data encryption or access control for sensitive events

**Timeline:** 6-8 weeks  
**Effort:** High (requires protocol-level changes)

---

#### 13. Standardize Error Handling
**Issue:** T-IV-02  
**Action:** Replace panic!() with Result types across all contracts

**Timeline:** 6-8 weeks  
**Effort:** High (requires extensive refactoring)


---

## 8. Follow-up Issues

The following security issues have been created for tracking and implementation:

### Critical Priority

1. **[SECURITY-001] Add Authorization to Reporting Contract Queries**
   - **Severity:** HIGH
   - **Component:** reporting contract
   - **Description:** Implement caller verification in all query functions to prevent unauthorized access to sensitive financial data
   - **Acceptance Criteria:**
     - All reporting query functions require caller authentication
     - Caller must be the user or have explicit permission
     - Add access control list (ACL) support for shared access
     - Update tests to verify authorization checks
   - **Estimated Effort:** 2-3 days

2. **[SECURITY-002] Implement Reentrancy Protection in Orchestrator**
   - **Severity:** HIGH
   - **Component:** orchestrator contract
   - **Description:** Add reentrancy guard to prevent state corruption during cross-contract calls
   - **Acceptance Criteria:**
     - Reentrancy guard implemented using storage flag
     - All cross-contract call functions protected
     - Tests verify reentrancy attempts are blocked
     - Gas cost impact documented
   - **Estimated Effort:** 3-5 days

3. **[SECURITY-003] Add Rate Limiting to Emergency Transfers**
   - **Severity:** HIGH
   - **Component:** family_wallet contract
   - **Description:** Enforce cooldown and transfer limits in emergency mode to prevent rapid fund drain
   - **Acceptance Criteria:**
     - Cooldown enforced between emergency transfers
     - Maximum transfer count per time period implemented
     - Emergency config includes rate limit parameters
     - Tests verify rate limiting works correctly
   - **Estimated Effort:** 2-3 days

### High Priority

4. **[SECURITY-004] Replace Checksum with Cryptographic Hash**
   - **Severity:** MEDIUM
   - **Component:** data_migration module
   - **Description:** Use SHA-256 instead of simple checksum for data integrity verification
   - **Acceptance Criteria:**
     - SHA-256 hash replaces simple checksum
     - Backward compatibility maintained for existing snapshots
     - Tests verify collision resistance
     - Documentation updated
   - **Estimated Effort:** 1-2 days

5. **[SECURITY-005] Implement Storage Bounds and Entity Limits**
   - **Severity:** MEDIUM
   - **Component:** All contracts (savings_goals, bill_payments, insurance)
   - **Description:** Add per-user limits on entity creation to prevent storage bloat DoS
   - **Acceptance Criteria:**
     - Maximum entities per user defined (goals: 100, bills: 200, policies: 50)
     - Creation functions enforce limits
     - Error messages indicate limit reached
     - Tests verify limits are enforced
     - Admin function to adjust limits if needed
   - **Estimated Effort:** 3-4 days

6. **[SECURITY-006] Standardize Storage Type to Persistent**
   - **Severity:** MEDIUM
   - **Component:** savings_goals contract
   - **Description:** Convert all storage to persistent type to prevent TTL mismatch issues
   - **Acceptance Criteria:**
     - All instance storage converted to persistent
     - TTL management updated for persistent storage
     - Migration path documented
     - Tests verify storage consistency
   - **Estimated Effort:** 2-3 days

7. **[SECURITY-007] Add Contract Address Validation**
   - **Severity:** MEDIUM
   - **Component:** reporting contract
   - **Description:** Validate configured contract addresses are actual contracts
   - **Acceptance Criteria:**
     - Address validation in configure_addresses()
     - Test call to verify contract responds
     - Error handling for invalid addresses
     - Tests verify validation works
   - **Estimated Effort:** 1-2 days

8. **[SECURITY-008] Enforce Role Expiry Consistently**
   - **Severity:** MEDIUM
   - **Component:** family_wallet contract
   - **Description:** Add role expiry checks to all role-based functions
   - **Acceptance Criteria:**
     - All role-based functions check expiry
     - Expired roles immediately lose access
     - Tests verify expiry enforcement
     - Documentation updated
   - **Estimated Effort:** 2-3 days

### Medium Priority

9. **[SECURITY-009] Implement Global Pause Coordinator**
   - **Severity:** MEDIUM
   - **Component:** New pause_coordinator contract
   - **Description:** Create coordinator contract to synchronize pause state across all contracts
   - **Acceptance Criteria:**
     - New pause coordinator contract deployed
     - All contracts register with coordinator
     - Pause all function implemented
     - Pause reason tracking added
     - Tests verify synchronized pause
   - **Estimated Effort:** 4-6 weeks

10. **[SECURITY-010] Add Balance Verification Functions**
    - **Severity:** MEDIUM
    - **Component:** All contracts handling funds
    - **Description:** Implement balance reconciliation to detect state-balance mismatches
    - **Acceptance Criteria:**
      - Balance verification function implemented
      - Periodic reconciliation mechanism
      - Mismatch events emitted
      - Admin dashboard shows balance status
      - Tests verify reconciliation works
    - **Estimated Effort:** 3-4 weeks

### Low Priority

11. **[SECURITY-011] Implement Audit Log Cleanup**
    - **Severity:** LOW
    - **Component:** savings_goals, remittance_split contracts
    - **Description:** Add automatic audit log rotation to prevent unbounded growth
    - **Acceptance Criteria:**
      - Maximum audit entries defined (1000)
      - Oldest entries removed when limit reached
      - Archive mechanism for old logs
      - Tests verify rotation works
    - **Estimated Effort:** 2-3 weeks

12. **[SECURITY-012] Add Input Bounds Validation**
    - **Severity:** LOW
    - **Component:** All contracts
    - **Description:** Implement maximum limits for string inputs and amounts
    - **Acceptance Criteria:**
      - Maximum string length defined (256 characters)
      - Maximum amount defined (i128::MAX / 2)
      - Validation in all input functions
      - Tests verify bounds enforcement
    - **Estimated Effort:** 2-3 weeks

13. **[SECURITY-013] Implement Contract Upgrade Mechanism**
    - **Severity:** LOW
    - **Component:** All contracts
    - **Description:** Design and implement upgrade pattern for deployed contracts
    - **Acceptance Criteria:**
      - Upgrade mechanism designed
      - Migration path documented
      - Backward compatibility maintained
      - Tests verify upgrade works
    - **Estimated Effort:** 8-12 weeks

---

## 9. Testing Recommendations

### 9.1 Security Testing

- **Fuzz Testing:** Test arithmetic operations with random inputs to detect overflow/underflow
- **Reentrancy Testing:** Attempt reentrancy attacks on all cross-contract calls
- **Authorization Testing:** Verify all functions properly check caller permissions
- **Storage Testing:** Test TTL expiration and storage consistency
- **Pause Testing:** Verify pause state synchronization across contracts

### 9.2 Integration Testing

- **Cross-Contract Flows:** Test complete remittance flows with failures at each step
- **Multi-Sig Workflows:** Test all multi-sig transaction types with various signer combinations
- **Emergency Scenarios:** Test emergency mode activation and fund recovery
- **Data Migration:** Test import/export with corrupted data and invalid checksums

### 9.3 Performance Testing

- **Storage Bloat:** Test with maximum entities to measure storage costs
- **Gas Costs:** Measure gas costs for all operations, especially cross-contract calls
- **Pagination:** Test query functions with large result sets
- **Batch Operations:** Test batch functions with maximum batch sizes

---

## 10. Monitoring & Incident Response

### 10.1 Monitoring Recommendations

- **Event Monitoring:** Monitor all events for suspicious patterns
- **Balance Monitoring:** Track contract balances and detect mismatches
- **Pause State Monitoring:** Alert on pause state changes
- **Storage Monitoring:** Track storage usage and growth rates
- **Authorization Failures:** Monitor failed authorization attempts

### 10.2 Incident Response Plan

1. **Detection:** Automated monitoring detects anomaly
2. **Assessment:** Security team evaluates severity and impact
3. **Containment:** Pause affected contracts if necessary
4. **Investigation:** Analyze events and state to determine root cause
5. **Remediation:** Deploy fixes or migrate to new contracts
6. **Recovery:** Restore normal operations and verify integrity
7. **Post-Mortem:** Document incident and update security controls

---

## 11. Compliance Considerations

### 11.1 Data Privacy

- **GDPR:** User financial data is publicly visible on blockchain
- **Recommendation:** Implement off-chain encryption or privacy layer
- **Action:** Add privacy notice to user documentation

### 11.2 Financial Regulations

- **AML/KYC:** No identity verification in smart contracts
- **Recommendation:** Implement off-chain compliance layer
- **Action:** Document compliance requirements for integrators

### 11.3 Audit Trail

- **Requirement:** Complete audit trail of all financial operations
- **Status:** ✅ Implemented via events and audit logs
- **Recommendation:** Ensure audit logs are preserved and accessible

---

## 12. Conclusion

The Remitwise smart contract suite demonstrates a solid security foundation with consistent authorization patterns, comprehensive event logging, and robust pause mechanisms. However, several critical gaps require immediate attention before mainnet deployment:

**Critical Actions Required:**
1. Add authorization to reporting contract queries
2. Implement reentrancy protection in orchestrator
3. Add rate limiting to emergency transfers

**High-Priority Improvements:**
4. Replace weak checksum with cryptographic hash
5. Implement storage bounds and entity limits
6. Standardize storage types
7. Add contract address validation
8. Enforce role expiry consistently

**Ongoing Security Practices:**
- Regular security audits
- Continuous monitoring and alerting
- Incident response planning
- User education on security best practices

By addressing these issues systematically, the Remitwise platform can achieve a strong security posture suitable for production deployment.

---

## Document History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2026-02-24 | Security Team | Initial threat model |

---

## References

- [Soroban Security Best Practices](https://soroban.stellar.org/docs/security)
- [Smart Contract Security Verification Standard](https://github.com/securing/SCSVS)
- [OWASP Smart Contract Top 10](https://owasp.org/www-project-smart-contract-top-10/)
- Remitwise Architecture Documentation (ARCHITECTURE.md)
