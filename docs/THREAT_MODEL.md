# Threat Model: Remitwise Contracts (Contributor-Focused)

## Overview
This document provides a STRIDE-style threat analysis for the Remitwise smart contracts, focusing on entrypoint-specific risks and mitigations. It's intended for contributors and reviewers to validate code against documented security expectations.

## STRIDE Framework
- **Spoofing**: Pretending to be someone else
- **Tampering**: Modifying data or code without permission
- **Repudiation**: Denying performing an action
- **Information Disclosure**: Exposing private information
- **Denial of Service**: Preventing legitimate use
- **Elevation of Privilege**: Gaining unauthorized permissions

## Entrypoint Threat Breakdown

### 1. Remittance Split Contract (`remittance_split`)

#### Entrypoint: `initialize_split`
| Threat | Description | Mitigation |
|--------|-------------|------------|
| Spoofing | Attacker initializes split for someone else's address | `owner.require_auth_for_args()` with domain-separated payload including network_id and contract_addr |
| Tampering | Overwriting existing split config | Checks `AlreadyInitialized` before storing new config |
| Repudiation | Owner denies initializing split | Event emitted with owner address and timestamp |
| Information Disclosure | N/A | Config is public (necessary for split calculations) |
| Denial of Service | N/A | Single initialization per contract |
| Elevation of Privilege | N/A | Only owner can initialize |

#### Entrypoint: `distribute_usdc`
| Threat | Description | Mitigation |
|--------|-------------|------------|
| Spoofing | Attacker distributes from someone else's address | `from.require_auth()` + checks `from == config.owner` |
| Tampering | Using malicious token contract | Validates `usdc_contract == config.usdc_contract` |
| Tampering | Self-transfer to bloat audit logs | Rejects if any destination == from address |
| Repudiation | Sender denies distribution | Event emitted with all details |
| Information Disclosure | N/A | Transfers are public on blockchain |
| Denial of Service | Replay attacks | Nonce-based replay protection |
| Denial of Service | Deadline abuse | Validates deadline is not in past and within max window |
| Elevation of Privilege | N/A | Only owner can distribute |

### 2. Savings Goals Contract (`savings_goals`)

#### Entrypoint: `create_goal`
| Threat | Description | Mitigation |
|--------|-------------|------------|
| Spoofing | Attacker creates goal for someone else | `owner.require_auth()` |
| Tampering | Creating goals with invalid parameters | Validates `target_amount > 0`, `target_date > now` |
| Repudiation | Owner denies creating goal | Event emitted with goal details |
| Information Disclosure | N/A | Goals are visible to owner only via queries |
| Denial of Service | Creating unlimited goals | (To be implemented: per-user entity limits) |
| Elevation of Privilege | N/A | Goal ownership tracked per goal |

### 3. Bill Payments Contract (`bill_payments`)

#### Entrypoint: `pay_bill`
| Threat | Description | Mitigation |
|--------|-------------|------------|
| Spoofing | Attacker pays someone else's bill | `caller.require_auth()` + checks `caller == bill.owner` |
| Tampering | Paying already paid bill | Checks bill is unpaid before processing |
| Repudiation | Payer denies paying bill | Event emitted with payment details |
| Information Disclosure | N/A | Bill payments are public |
| Denial of Service | Replaying payment | Bill marked as paid atomically |
| Elevation of Privilege | N/A | Only bill owner can pay |

### 4. Orchestrator Contract (`orchestrator`)

#### Entrypoint: `execute_remittance_flow`
| Threat | Description | Mitigation |
|--------|-------------|------------|
| Spoofing | Attacker triggers flow for someone else | `caller.require_auth()` + checks against stored owner |
| Tampering | Reentrancy attacks | Execution state lock (`Idle`/`Executing`) guards entrypoint |
| Repudiation | Caller denies triggering flow | Audit log entry + events emitted |
| Information Disclosure | N/A | Flow details are public |
| Denial of Service | Replay attacks | Per-caller per-command nonce tracking |
| Elevation of Privilege | N/A | Flow restricted to authorized callers |

### 5. Family Wallet Contract (`family_wallet`)

#### Entrypoint: `execute_emergency_transfer_now`
| Threat | Description | Mitigation |
|--------|-------------|------------|
| Spoofing | Attacker triggers emergency transfer | `caller.require_auth()` + emergency mode check |
| Tampering | Rapid fund drain | Cooldown timer between emergency transfers |
| Repudiation | Admin denies emergency transfer | Event emitted with all details |
| Information Disclosure | N/A | Transfers are public |
| Denial of Service | N/A | Emergency mode is time-bound |
| Elevation of Privilege | N/A | Only emergency admins can trigger |

## Common Mitigations Across Contracts

### Authorization
- All state-modifying functions use `require_auth()`
- Owner-based access control for per-entity operations
- Role-based access control in family wallet (Owner > Admin > Member > Viewer)

### Replay Protection
- Nonce-based replay protection for signed operations
- Domain-separated payloads to prevent cross-contract replay

### Pause Mechanism
- Global pause switch to halt operations during incidents
- Pause admin role separation from upgrade admin
- Timelock for unpause in emergency_killswitch

### Event Logging
- All state changes emit structured events
- Events include timestamps and actor addresses for auditability

### Input Validation
- Arithmetic operations use checked_* methods to prevent overflow/underflow
- Percentage sums validated to be exactly 100
- Token contract addresses validated against trusted values

## Testing Recommendations for Contributors
When modifying entrypoints, ensure you test:
1. Authorization bypass attempts
2. Replay attacks with old nonces
3. Invalid input parameters
4. Pause state enforcement
5. Cross-contract call failures (use try_invoke() pattern)

## References
- Root threat model: [THREAT_MODEL.md](../THREAT_MODEL.md)
- Authorization matrix: [AUTHORIZATION_MATRIX.md](AUTHORIZATION_MATRIX.md)
- Security review checklist: [SECURITY_REVIEW.md](SECURITY_REVIEW.md)
