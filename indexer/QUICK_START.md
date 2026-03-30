# Quick Start Guide

Get the Remitwise indexer running in 5 minutes.

## Prerequisites

- Node.js 18+
- Deployed Remitwise contracts (testnet or localnet)
- Contract addresses

## Setup

### 1. Install Dependencies

```bash
cd indexer
npm install
```

### 2. Configure Environment

```bash
cp .env.example .env
```

Edit `.env` with your settings:

```env
# For Testnet
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
NETWORK_PASSPHRASE=Test SDF Network ; September 2015

# For Localnet
# STELLAR_RPC_URL=http://localhost:8000/soroban/rpc
# NETWORK_PASSPHRASE=Standalone Network ; February 2017

# Your deployed contract addresses
BILL_PAYMENTS_CONTRACT=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
SAVINGS_GOALS_CONTRACT=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
INSURANCE_CONTRACT=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

# Optional
REMITTANCE_SPLIT_CONTRACT=CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

# Database location
DB_PATH=./data/remitwise.db

# Polling interval (milliseconds)
POLL_INTERVAL_MS=5000

# Start from ledger (0 = from beginning)
START_LEDGER=0
```

### 3. Build

```bash
npm run build
```

### 4. Run

```bash
npm start
```

You should see:
```
Remitwise Indexer v1.0.0

Initializing database: ./data/remitwise.db
Database schema initialized
Starting indexer...
Monitoring contracts: [...]
Processing ledgers 1000 to 1050
Found 5 events for contract CXXXXXXX...
```

## Query Data

Open a new terminal while the indexer is running:

### User Dashboard
```bash
npm start query dashboard GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

### Overdue Bills
```bash
npm start query overdue
```

### Filter by Tag
```bash
npm start query tag utilities
```

### List All Tags
```bash
npm start query tags
```

### Active Goals
```bash
npm start query goals
```

## Docker (Alternative)

### Using Docker Compose

```bash
# Configure .env first
cp .env.example .env
# Edit .env

# Start
docker-compose up -d

# View logs
docker-compose logs -f indexer

# Stop
docker-compose down
```

## Testing with Localnet

### 1. Start Stellar Localnet

```bash
stellar network start local
```

### 2. Deploy Contracts

```bash
cd ..
./scripts/deploy_local.sh
```

### 3. Configure Indexer

```bash
cd indexer
cp .env.example .env
```

Edit `.env`:
```env
STELLAR_RPC_URL=http://localhost:8000/soroban/rpc
NETWORK_PASSPHRASE=Standalone Network ; February 2017
START_LEDGER=1

# Use contract addresses from deployment output
BILL_PAYMENTS_CONTRACT=...
SAVINGS_GOALS_CONTRACT=...
INSURANCE_CONTRACT=...
```

### 4. Run Indexer

```bash
npm start
```

### 5. Generate Test Events

In another terminal:

```bash
# Create a savings goal
stellar contract invoke \
  --id $SAVINGS_GOALS_CONTRACT \
  --source alice \
  -- create_goal \
  --caller alice \
  --name "Emergency Fund" \
  --target_amount 10000 \
  --target_date 1735689600

# Create a bill
stellar contract invoke \
  --id $BILL_PAYMENTS_CONTRACT \
  --source alice \
  -- create_bill \
  --caller alice \
  --name "Electricity" \
  --amount 150 \
  --due_date 1735689600 \
  --recurring false
```

### 6. Query Indexed Data

```bash
npm start query dashboard GXXXXXXX...
```

## Common Issues

### "Missing required environment variables"

Make sure `.env` exists and contains all required variables:
- STELLAR_RPC_URL
- BILL_PAYMENTS_CONTRACT
- SAVINGS_GOALS_CONTRACT
- INSURANCE_CONTRACT

### "Cannot connect to RPC"

- Check STELLAR_RPC_URL is correct
- For localnet, ensure `stellar network start local` is running
- For testnet, check your internet connection

### "No events found"

- Verify contracts are deployed and addresses are correct
- Check START_LEDGER is before contract deployment
- Ensure contracts have emitted events (create test data)

### Database locked

- Only run one indexer instance per database
- Stop other instances: `pkill -f "node.*indexer"`

## Next Steps

- Read [README.md](README.md) for detailed documentation
- Check [IMPLEMENTATION.md](IMPLEMENTATION.md) for technical details
- Review [examples/query-examples.ts](examples/query-examples.ts) for API usage
- Explore database schema in [src/db/schema.ts](src/db/schema.ts)

## Stop Indexer

Press `Ctrl+C` to gracefully shutdown.

The indexer will:
1. Stop polling for new events
2. Save the last processed ledger
3. Close database connections
4. Exit cleanly

When restarted, it will resume from the last processed ledger.
