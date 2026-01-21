# Remittance Split Contract

Soroban smart contract for automatically distributing remittances across multiple financial categories.

## Overview

The Remittance Split contract coordinates the distribution of incoming remittances:

- Configure percentage splits across categories
- Calculate distribution amounts
- Support customizable allocation preferences
- Provide default allocation strategy

## Key Features

### ✓ Flexible Configuration

- Set custom distribution percentages
- Support for 4 financial categories
- Percentage validation (must sum to 100)
- Default configuration provided

### ✓ Automatic Calculation

- Calculate split amounts from total
- Handle rounding (remainder to insurance)
- Efficient percentage-based math
- Support large amounts

### ✓ Category Support

- Spending (daily expenses)
- Savings (long-term goals)
- Bills (payment obligations)
- Insurance (coverage premiums)

## Data Structure

### Split Configuration

```
Vec<u32> representing [
    spending_percent,      // For daily expenses
    savings_percent,       // For savings goals
    bills_percent,         // For bill payments
    insurance_percent      // For insurance premiums
]

Example: [50, 30, 15, 5]
- 50% spending
- 30% savings
- 15% bills
- 5% insurance
```

## API Reference

### initialize_split

Configure remittance distribution percentages.

```rust
pub fn initialize_split(
    env: Env,
    spending_percent: u32,
    savings_percent: u32,
    bills_percent: u32,
    insurance_percent: u32,
) -> bool
```

**Parameters:**

- `spending_percent`: Daily spending allocation (0-100)
- `savings_percent`: Savings allocation (0-100)
- `bills_percent`: Bill payment allocation (0-100)
- `insurance_percent`: Insurance allocation (0-100)

**Returns:** `true` if valid (sums to 100), `false` otherwise

**Constraint:** All percentages must sum to exactly 100

**Example:**

```rust
let success = RemittanceSplit::initialize_split(
    env,
    50,   // 50% spending
    30,   // 30% savings
    15,   // 15% bills
    5,    // 5% insurance
);
assert!(success);
```

### get_split

Retrieve current split configuration.

```rust
pub fn get_split(env: Env) -> Vec<u32>
```

**Returns:** `Vec<u32>` with percentages [spending, savings, bills, insurance]

**Default:** Returns [50, 30, 15, 5] if not configured

**Example:**

```rust
let split = RemittanceSplit::get_split(env);
println!("Spending: {}%", split.get(0).unwrap());
println!("Savings: {}%", split.get(1).unwrap());
println!("Bills: {}%", split.get(2).unwrap());
println!("Insurance: {}%", split.get(3).unwrap());
```

### calculate_split

Calculate distribution amounts from total remittance.

```rust
pub fn calculate_split(env: Env, total_amount: i128) -> Vec<i128>
```

**Parameters:**

- `total_amount`: Total remittance amount in stroops

**Returns:** `Vec<i128>` with amounts [spending, savings, bills, insurance]

**Rounding:** Remainder from rounding allocated to insurance category

**Example:**

```rust
let total = 100_000_000;  // 10 USDC (8 decimals)

let distribution = RemittanceSplit::calculate_split(env, total);

let spending = distribution.get(0).unwrap();   // 50_000_000
let savings = distribution.get(1).unwrap();    // 30_000_000
let bills = distribution.get(2).unwrap();      // 15_000_000
let insurance = distribution.get(3).unwrap();  // 5_000_000
```

## Usage Examples

### Setup Default Split

```rust
// 50% spending, 30% savings, 15% bills, 5% insurance
let success = RemittanceSplit::initialize_split(env, 50, 30, 15, 5);
assert!(success);

// Verify configuration
let split = RemittanceSplit::get_split(env);
assert_eq!(split.get(0).unwrap(), 50);  // spending
```

### Conservative Split (More Savings)

```rust
// After marriage: increase savings, reduce bills
let success = RemittanceSplit::initialize_split(
    env,
    40,   // 40% spending (reduced)
    40,   // 40% savings (increased)
    15,   // 15% bills (same)
    5,    // 5% insurance (same)
);
assert!(success);
```

### Emergency Focused (More Insurance)

```rust
// During emergency: increase insurance/emergency fund
let success = RemittanceSplit::initialize_split(
    env,
    30,   // 30% spending (reduced)
    20,   // 20% savings (reduced)
    15,   // 15% bills (same)
    35,   // 35% insurance/emergency (increased)
);
assert!(success);
```

### Calculate Distribution

```rust
// Receive 1,000 USDC remittance
let remittance = 100_000_000_000;  // 1,000 USDC

// Calculate distribution
let distribution = RemittanceSplit::calculate_split(env, remittance);

// Extract amounts
let spending_amt = distribution.get(0).unwrap();    // 500 USDC
let savings_amt = distribution.get(1).unwrap();     // 300 USDC
let bills_amt = distribution.get(2).unwrap();       // 150 USDC
let insurance_amt = distribution.get(3).unwrap();   // 50 USDC

// Verify total
let total = spending_amt + savings_amt + bills_amt + insurance_amt;
assert_eq!(total, remittance);
```

### Full Remittance Processing

```rust
// 1. Configure split
RemittanceSplit::initialize_split(env, 50, 30, 15, 5);

// 2. Receive 1,000 USDC
let remittance = 100_000_000_000;

// 3. Calculate distribution
let dist = RemittanceSplit::calculate_split(env, remittance);

// 4. Route to other contracts
let spending = dist.get(0).unwrap();
let savings = dist.get(1).unwrap();
let bills = dist.get(2).unwrap();
let insurance = dist.get(3).unwrap();

// 5. Execute transfers
FamilyWallet::update_balance(spending_wallet, spending);
SavingsGoals::add_to_goal(primary_goal, savings);
BillPayments::pay_bills(bills);
Insurance::pay_premiums(insurance);
```

## Integration Points

### Entry Point for Remittances

Remittance Split is the first contract called when processing incoming money:

```
Incoming Remittance
        │
        ▼
RemittanceSplit.calculate_split()
        │
        ├─ BillPayments (bills allocation)
        ├─ SavingsGoals (savings allocation)
        ├─ Insurance (insurance allocation)
        └─ FamilyWallet (spending allocation)
```

### With Other Contracts

- **BillPayments**: Receives bills allocation
- **Insurance**: Receives insurance allocation
- **SavingsGoals**: Receives savings allocation
- **FamilyWallet**: Receives spending allocation

## Use Cases

### Use Case 1: New Remittance Recipient

**Goal**: Set up initial financial plan

```rust
// Start with conservative split
RemittanceSplit::initialize_split(env, 30, 50, 15, 5);

// Focus on: emergency fund, savings, basic bills
```

### Use Case 2: Growing Family

**Goal**: Balance family growth with financial stability

```rust
// Adjust split as family grows
RemittanceSplit::initialize_split(env, 40, 30, 20, 10);

// More for: bills (family expenses), insurance (family protection)
// Less for: savings (immediate needs)
```

### Use Case 3: Health Emergency

**Goal**: Prioritize emergency coverage

```rust
// Temporary adjustment during crisis
RemittanceSplit::initialize_split(env, 20, 10, 15, 55);

// Most funds: emergency/insurance coverage
// Less: regular spending and savings
```

### Use Case 4: Education Investment

**Goal**: Fund education goals

```rust
// Allocate more to education savings
RemittanceSplit::initialize_split(env, 30, 50, 10, 10);

// More for: savings (education funds)
// Less for: bills, insurance
```

## Best Practices

### 1. Percentages Must Sum to 100

```rust
// Valid
RemittanceSplit::initialize_split(env, 50, 30, 15, 5);  // ✓ 100%

// Invalid - will return false
RemittanceSplit::initialize_split(env, 50, 30, 15, 4);  // ✗ 99%
```

### 2. Default Fallback

```rust
// If not configured, default used
let split = RemittanceSplit::get_split(env);
// Returns: [50, 30, 15, 5] (default)

// After explicit setup
RemittanceSplit::initialize_split(env, 40, 40, 15, 5);
let split = RemittanceSplit::get_split(env);
// Returns: [40, 40, 15, 5]
```

### 3. Round-Trip Consistency

```rust
// Same split produces same distribution
let split = RemittanceSplit::get_split(env);
let dist1 = RemittanceSplit::calculate_split(env, 1_000_000);
let dist2 = RemittanceSplit::calculate_split(env, 1_000_000);

assert_eq!(dist1, dist2);  // Always same
```

### 4. Rounding Behavior

```rust
// Remainder goes to insurance
let total = 1_000_000_001;  // 1 extra stroop
let dist = RemittanceSplit::calculate_split(env, total);

let sum = dist.get(0).unwrap()
    + dist.get(1).unwrap()
    + dist.get(2).unwrap()
    + dist.get(3).unwrap();

assert_eq!(sum, total);  // Insurance receives the extra stroop
```

## Security Considerations

1. **Percentage Validation**: Percentages strictly validated (must sum to 100)
2. **No Negative Values**: Percentages must be 0-100
3. **Deterministic Math**: Same input always produces same output
4. **No State Mutation**: Only reads/writes own configuration
5. **Calculation Transparency**: Distribution calculations are verifiable

## Testing

```rust
#[test]
fn test_initialize_valid() {
    let env = Env::default();
    let success = RemittanceSplit::initialize_split(env, 50, 30, 15, 5);
    assert!(success);
}

#[test]
fn test_initialize_invalid_sum() {
    let env = Env::default();
    let success = RemittanceSplit::initialize_split(env, 50, 30, 15, 4);
    assert!(!success);  // 99, not 100
}

#[test]
fn test_calculate_split() {
    let env = Env::default();
    RemittanceSplit::initialize_split(env, 50, 30, 15, 5);

    let dist = RemittanceSplit::calculate_split(env, 100_000_000);

    assert_eq!(dist.get(0).unwrap(), 50_000_000);   // 50%
    assert_eq!(dist.get(1).unwrap(), 30_000_000);   // 30%
    assert_eq!(dist.get(2).unwrap(), 15_000_000);   // 15%
    assert_eq!(dist.get(3).unwrap(), 5_000_000);    // 5%
}

#[test]
fn test_default_split() {
    let env = Env::default();
    let split = RemittanceSplit::get_split(env);

    assert_eq!(split.get(0).unwrap(), 50);
    assert_eq!(split.get(1).unwrap(), 30);
    assert_eq!(split.get(2).unwrap(), 15);
    assert_eq!(split.get(3).unwrap(), 5);
}
```

## Deployment

### Compile

```bash
cd remittance_split
cargo build --release --target wasm32-unknown-unknown
```

### Deploy

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/remittance_split.wasm \
  --network testnet \
  --identity deployer
```

## Contract Size

- **Unoptimized**: ~65KB
- **Optimized**: ~32KB

## Gas Costs (Estimate)

- Initialize split: ~2,000 stroops
- Calculate split: ~1,500 stroops
- Get split: ~1,000 stroops

## Error Scenarios

### Invalid Percentage Sum

```rust
let success = RemittanceSplit::initialize_split(env, 50, 30, 15, 6);
// Returns: false (sum = 101, not 100)
```

### Negative Percentages

```rust
// Soroban won't allow negative u32, caught at compilation
// Rust: error: cannot have negative value for u32
```

### After Failed Initialize

```rust
let success = RemittanceSplit::initialize_split(env, 50, 30, 15, 6);
assert!(!success);

// Configuration unchanged (still default)
let split = RemittanceSplit::get_split(env);
assert_eq!(split.get(0).unwrap(), 50);  // Still default
```

## Future Enhancements

- [ ] Multiple split profiles (save multiple configurations)
- [ ] Scheduled split changes (automatic adjustment on date)
- [ ] Progressive splits (change over time)
- [ ] Split recommendations based on financial metrics
- [ ] A/B testing of splits
- [ ] Historical split tracking
- [ ] Split validation against household composition

## References

- [Full API Reference](../docs/API_REFERENCE.md#remittance-split-contract)
- [Usage Examples](../docs/USAGE_EXAMPLES.md#remittance-split-examples)
- [Architecture Overview](../docs/ARCHITECTURE.md)
- [Deployment Guide](../docs/DEPLOYMENT_GUIDE.md)

## Support

For questions or support:

- [Remitwise GitHub](https://github.com/Remitwise-Org/Remitwise-Contracts)
- [Stellar Documentation](https://developers.stellar.org/)
