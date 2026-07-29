# RemitWise CLI

A command-line interface for interacting with RemitWise smart contracts on Soroban.

## Prerequisites

- Soroban CLI installed (`cargo install soroban-cli`)
- Contracts deployed on the network
- Environment variables set

## Environment Variables

Set the following environment variables:

- `SOROBAN_NETWORK`: Network to use (`local` for localnet, `testnet` for testnet)
- `REMITTANCE_SPLIT_CONTRACT_ID`: Contract ID for remittance split
- `SAVINGS_GOALS_CONTRACT_ID`: Contract ID for savings goals
- `BILL_PAYMENTS_CONTRACT_ID`: Contract ID for bill payments
- `INSURANCE_CONTRACT_ID`: Contract ID for insurance
- `OWNER_ADDRESS`: Your address for operations requiring authentication

## Building

```bash
cargo build --release --bin remitwise-cli
```

## Usage

```bash
./target/release/remitwise-cli --help
```

### Commands

#### Split Commands

- `split get-config`: Get the current split configuration

#### Goals Commands

- `goals list`: List all savings goals for the owner
- `goals create <name> <target_amount> <target_date>`: Create a new savings goal

#### Bills Commands

- `bills list`: List unpaid bills for the owner
- `bills pay <bill_id>`: Pay a specific bill

#### Insurance Commands

- `insurance list`: List active insurance policies for the owner

## Network Setup

### Localnet

Start local Soroban network:

```bash
soroban dev start
export SOROBAN_NETWORK=local
```

Deploy contracts and note their IDs.

### Testnet

Use the public testnet:

```bash
export SOROBAN_NETWORK=testnet
```

Ensure contracts are deployed on testnet and set the CONTRACT_ID variables.

## Example Session

```bash
# Set environment
export SOROBAN_NETWORK=local
export OWNER_ADDRESS=G...
export REMITTANCE_SPLIT_CONTRACT_ID=C...
# etc.

# Get split config
./target/release/remitwise-cli split get-config

# List goals
./target/release/remitwise-cli goals list

# Create a goal
./target/release/remitwise-cli goals create "Vacation Fund" 500000 1735689600

# List bills
./target/release/remitwise-cli bills list

# Pay bill
./target/release/remitwise-cli bills pay 1
```