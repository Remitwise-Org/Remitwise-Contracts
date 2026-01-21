# Insurance Contract

Soroban smart contract for managing insurance policies with monthly premium payments and coverage tracking.

## Overview

The Insurance contract provides comprehensive policy management:

- Create insurance policies with coverage details
- Track monthly premium payments
- Manage policy activation status
- Calculate total insurance obligations

## Key Features

### ✓ Policy Management

- Create policies with coverage types
- Configurable premium and coverage amounts
- Policy activation/deactivation
- Payment schedule tracking

### ✓ Premium Payments

- Monthly premium payment recording
- Automatic payment date advancement
- Multi-policy support
- Total premium calculation

### ✓ Coverage Tracking

- Health insurance policies
- Emergency coverage
- Education policies
- Custom coverage types

## Data Structure

### InsurancePolicy

```rust
pub struct InsurancePolicy {
    pub id: u32,                // Unique identifier
    pub name: String,           // Policy name
    pub coverage_type: String,  // "health", "emergency", etc.
    pub monthly_premium: i128,  // Monthly cost in stroops
    pub coverage_amount: i128,  // Total coverage in stroops
    pub active: bool,           // Policy status
    pub next_payment_date: u64, // Unix timestamp
}
```

## API Reference

### create_policy

Create a new insurance policy.

```rust
pub fn create_policy(
    env: Env,
    name: String,
    coverage_type: String,
    monthly_premium: i128,
    coverage_amount: i128,
) -> u32
```

**Parameters:**

- `name`: Policy name (e.g., "Family Health Insurance")
- `coverage_type`: Coverage type (e.g., "health", "emergency")
- `monthly_premium`: Monthly premium in stroops
- `coverage_amount`: Total coverage amount in stroops

**Returns:** Policy ID

**Notes:** First payment due 30 days from creation

**Example:**

```rust
let policy_id = Insurance::create_policy(
    env,
    String::from_small_str("Health Insurance"),
    String::from_small_str("health"),
    50_000_000,     // 5 USDC/month
    1_000_000_000,  // 100 USDC coverage
);
```

### pay_premium

Pay monthly premium for a policy.

```rust
pub fn pay_premium(env: Env, policy_id: u32) -> bool
```

**Parameters:**

- `policy_id`: ID of policy to pay

**Returns:** `true` if successful, `false` if inactive or not found

**Example:**

```rust
if Insurance::pay_premium(env, policy_id) {
    println!("Premium paid, next payment due in 30 days");
}
```

### get_policy

Retrieve a specific policy.

```rust
pub fn get_policy(env: Env, policy_id: u32) -> Option<InsurancePolicy>
```

**Returns:** `Some(InsurancePolicy)` if found

### get_active_policies

Get all active policies.

```rust
pub fn get_active_policies(env: Env) -> Vec<InsurancePolicy>
```

**Returns:** Vector of active policies

### get_total_monthly_premium

Calculate total monthly insurance cost.

```rust
pub fn get_total_monthly_premium(env: Env) -> i128
```

**Returns:** Total premium in stroops

**Example:**

```rust
let total = Insurance::get_total_monthly_premium(env);
// Budget: total / 100_000_000 USDC per month
```

### deactivate_policy

Deactivate an insurance policy.

```rust
pub fn deactivate_policy(env: Env, policy_id: u32) -> bool
```

**Parameters:**

- `policy_id`: ID of policy to deactivate

**Returns:** `true` if successful

**Example:**

```rust
// Cancel policy
Insurance::deactivate_policy(env, policy_id);

// Total premium now reduced
let new_total = Insurance::get_total_monthly_premium(env);
```

## Usage Examples

### Create Multiple Policies

```rust
// Health insurance
let health = Insurance::create_policy(
    env,
    String::from_small_str("Family Health"),
    String::from_small_str("health"),
    50_000_000,     // 5 USDC/month
    1_000_000_000,  // 100 USDC coverage
);

// Emergency fund insurance
let emergency = Insurance::create_policy(
    env,
    String::from_small_str("Emergency Coverage"),
    String::from_small_str("emergency"),
    20_000_000,     // 2 USDC/month
    500_000_000,    // 50 USDC coverage
);

// Education savings plan
let education = Insurance::create_policy(
    env,
    String::from_small_str("Education Plan"),
    String::from_small_str("education"),
    30_000_000,     // 3 USDC/month
    300_000_000,    // 30 USDC coverage
);
```

### Monthly Premium Payment

```rust
// Get all active policies
let policies = Insurance::get_active_policies(env);

// Pay all premiums
for policy in policies.iter() {
    if Insurance::pay_premium(env, policy.id) {
        println!("Paid premium for {}", policy.name);
    }
}

// Verify payments
let total = Insurance::get_total_monthly_premium(env);
// total = 100_000_000 (10 USDC)
```

### Insurance Budget Planning

```rust
// Calculate annual insurance cost
let monthly_premium = Insurance::get_total_monthly_premium(env);
let annual_cost = monthly_premium * 12;

println!("Monthly: {} USDC", monthly_premium / 100_000_000);
println!("Annual: {} USDC", annual_cost / 100_000_000);

// Factor into budget
let remittance = 1_000_000_000;  // 100 USDC
let insurance_percent = (monthly_premium * 100) / remittance;
println!("Insurance: {}% of remittance", insurance_percent);
```

### Policy Cancellation and Update

```rust
// Review policies
let policies = Insurance::get_active_policies(env);
for policy in policies.iter() {
    println!("{}: {} stroops/month", policy.name, policy.monthly_premium);
}

// Cancel low-value policy
Insurance::deactivate_policy(env, low_value_policy_id);

// New budget without cancelled policy
let new_total = Insurance::get_total_monthly_premium(env);
```

## Integration Points

### With Remittance Split

- Insurance receives allocation from split
- Premium payment from allocated funds
- Contributes to monthly budget calculations

### With Bill Payments

- Insurance premiums tracked as obligations
- Prioritized alongside bill payments
- Integrated in financial planning

### With Savings Goals

- Education policy complements savings goal
- Multiple coverage types support different goals
- Policy coverage provides safety net

### With Family Wallet

- Insurance for entire family
- Premium allocation per family member (future)
- Spending limits prevent insurance premium default

## Coverage Types

Common coverage type values:

| Type         | Purpose           | Example                          |
| ------------ | ----------------- | -------------------------------- |
| `health`     | Medical coverage  | Hospitalization, doctor visits   |
| `emergency`  | Emergency fund    | Unexpected expenses              |
| `education`  | Education savings | School fees, university          |
| `life`       | Life insurance    | Death benefit                    |
| `disability` | Income protection | Loss of income                   |
| `accident`   | Accident coverage | Medical treatment after accident |

## Best Practices

### 1. Multi-Policy Coverage

```rust
// Create diverse coverage
let health = Insurance::create_policy(
    env, health_name, "health", health_prem, health_cov
);
let emergency = Insurance::create_policy(
    env, emerg_name, "emergency", emerg_prem, emerg_cov
);
// Provides layered protection
```

### 2. Automatic Payment Processing

```rust
// During remittance processing
let insurance_allocation = split_result[3];
let policies = Insurance::get_active_policies(env);

for policy in policies.iter() {
    if insurance_allocation >= policy.monthly_premium {
        Insurance::pay_premium(env, policy.id);
        insurance_allocation -= policy.monthly_premium;
    }
}
```

### 3. Budget Forecasting

```rust
// Calculate future costs
let policies = Insurance::get_active_policies(env);
let monthly_cost = Insurance::get_total_monthly_premium(env);
let quarterly_cost = monthly_cost * 3;
let annual_cost = monthly_cost * 12;

println!("Monthly: {}", monthly_cost / 100_000_000);
println!("Quarterly: {}", quarterly_cost / 100_000_000);
println!("Annual: {}", annual_cost / 100_000_000);
```

## Security Considerations

1. **Immutable Policies**: Once created, policy terms don't change
2. **Activation Control**: Only deactivate, never modify terms
3. **Premium Certainty**: Amounts fixed at creation
4. **No Manual Overrides**: Payments are deterministic
5. **Clear State**: Active/inactive status always clear

## Testing

```rust
#[test]
fn test_create_policy() {
    let env = Env::default();

    let id = Insurance::create_policy(
        env,
        String::from_small_str("Health"),
        String::from_small_str("health"),
        50_000_000,
        1_000_000_000,
    );
    assert_eq!(id, 1);
}

#[test]
fn test_payment_advances_date() {
    let env = Env::default();

    let id = Insurance::create_policy(env, name, "health", premium, coverage);
    let policy1 = Insurance::get_policy(env, id).unwrap();
    let initial_date = policy1.next_payment_date;

    Insurance::pay_premium(env, id);
    let policy2 = Insurance::get_policy(env, id).unwrap();
    let new_date = policy2.next_payment_date;

    // Next payment should be 30 days later
    assert_eq!(new_date, initial_date + 2592000);  // 30 days in seconds
}

#[test]
fn test_inactive_policy_no_payment() {
    let env = Env::default();

    let id = Insurance::create_policy(env, name, type, premium, coverage);
    Insurance::deactivate_policy(env, id);

    let success = Insurance::pay_premium(env, id);
    assert!(!success);
}
```

## Deployment

### Compile

```bash
cd insurance
cargo build --release --target wasm32-unknown-unknown
```

### Deploy

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/insurance.wasm \
  --network testnet \
  --identity deployer
```

## Contract Size

- **Unoptimized**: ~85KB
- **Optimized**: ~42KB

## Gas Costs (Estimate)

- Create policy: ~4,500 stroops
- Pay premium: ~3,000 stroops
- Get active policies: ~2,000 stroops
- Deactivate policy: ~2,500 stroops

## Error Scenarios

### Pay Inactive Policy

```rust
let id = Insurance::create_policy(env, name, type, prem, cov);
Insurance::deactivate_policy(env, id);
let result = Insurance::pay_premium(env, id);
// Returns: false (policy inactive)
```

### Pay Nonexistent Policy

```rust
let result = Insurance::pay_premium(env, 999);
// Returns: false (not found)
```

### Deactivate Already Inactive

```rust
let id = Insurance::create_policy(env, name, type, prem, cov);
Insurance::deactivate_policy(env, id);  // First call
let policy = Insurance::get_policy(env, id).unwrap();
assert_eq!(policy.active, false);  // Already inactive
```

## Future Enhancements

- [ ] Claims management
- [ ] Policy modification (premium/coverage changes)
- [ ] Multiple beneficiaries
- [ ] Coverage limits per claim type
- [ ] Automated renewal
- [ ] Policy comparison and recommendations
- [ ] Premium discounts for loyalty
- [ ] Integration with external claims providers

## References

- [Full API Reference](../docs/API_REFERENCE.md#insurance-contract)
- [Usage Examples](../docs/USAGE_EXAMPLES.md#insurance-examples)
- [Architecture Overview](../docs/ARCHITECTURE.md)
- [Deployment Guide](../docs/DEPLOYMENT_GUIDE.md)

## Support

For questions or issues:

- [Remitwise GitHub](https://github.com/Remitwise-Org/Remitwise-Contracts)
- [Stellar Documentation](https://developers.stellar.org/)
