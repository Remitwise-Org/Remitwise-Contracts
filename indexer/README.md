# Remitwise Event Indexer

A minimal TypeScript-based indexer that consumes events from Remitwise smart contracts and builds an off-chain queryable database.

## Overview

This indexer monitors Soroban smart contracts on the Stellar network, processes emitted events, and stores normalized data in a SQLite database. It provides a simple query API for accessing indexed data.

## Features

- **Event Monitoring**: Continuously polls Stellar RPC for contract events
- **Data Normalization**: Parses and stores structured data from events
- **SQLite Storage**: Lightweight, file-based database for indexed data
- **Query API**: Simple interface for querying indexed entities
- **Tag Support**: Full support for tagging system across all entities
- **Graceful Shutdown**: Handles SIGINT/SIGTERM for clean shutdowns

## Architecture

```
┌─────────────────┐
│ Stellar Network │
│  (Testnet/Main) │
└────────┬────────┘
         │ Events
         ▼
┌─────────────────┐
│  Event Indexer  │
│   (TypeScript)  │
└────────┬────────┘
         │ Parsed Data
         ▼
┌─────────────────┐
│ SQLite Database │
│  (Normalized)   │
└────────┬────────┘
         │ Queries
         ▼
┌─────────────────┐
│   Query API     │
│  (CLI/HTTP)     │
└─────────────────┘
```

## Supported Contracts

- **Bill Payments**: Tracks bills, payments, and schedules
- **Savings Goals**: Monitors goals, deposits, and withdrawals
- **Insurance**: Indexes policies and premium payments
- **Remittance Split**: Records split transactions

## Prerequisites

- Node.js 18+ and npm
- Access to Stellar RPC endpoint (testnet or mainnet)
- Deployed Remitwise contract addresses

## Installation

1. **Install dependencies**:
```bash
cd indexer
npm install
```

2. **Configure environment**:
```bash
cp .env.example .env
```

Edit `.env` and set your configuration:
```env
# Stellar Network
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
NETWORK_PASSPHRASE=Test SDF Network ; September 2015

# Contract Addresses (from your deployments)
BILL_PAYMENTS_CONTRACT=CXXXXXXXXX...
SAVINGS_GOALS_CONTRACT=CXXXXXXXXX...
INSURANCE_CONTRACT=CXXXXXXXXX...
REMITTANCE_SPLIT_CONTRACT=CXXXXXXXXX...

# Database
DB_PATH=./data/remitwise.db

# Indexer Settings
POLL_INTERVAL_MS=5000
START_LEDGER=0
```

3. **Build the project**:
```bash
npm run build
```

## Usage

### Running the Indexer

Start the indexer to begin monitoring and indexing events:

```bash
npm start
```

The indexer will:
1. Initialize the SQLite database (if not exists)
2. Connect to the Stellar RPC endpoint
3. Start polling for events from configured contracts
4. Process and store events in the database
5. Continue running until stopped (Ctrl+C)

### Querying Indexed Data

The indexer includes a CLI query interface:

#### User Dashboard
View all data for a specific user:
```bash
npm start query dashboard GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
```

Output:
```
=== User Dashboard ===
Owner: GXXXXXXX...

Totals:
  Savings Goals: 3 (Total: 15000)
  Unpaid Bills: 2 (Total: 500)
  Active Policies: 1 (Coverage: 100000)

Savings Goals:
  [1] Emergency Fund: 5000/10000 [emergency, priority]
  [2] Vacation: 3000/5000 [travel, leisure]
  [3] New Car: 7000/20000 [vehicle]

Unpaid Bills:
  [1] Electricity: 150 (Due: 2026-03-01) [utilities, monthly]
  [2] Internet: 80 (Due: 2026-03-05) [utilities, monthly]

Active Policies:
  [1] Health Insurance (Medical): 100000 [health, family]
```

#### Overdue Bills
List all overdue bills across all users:
```bash
npm start query overdue
```

#### Entities by Tag
Find all entities with a specific tag:
```bash
npm start query tag utilities
```

Output:
```
=== Entities Tagged: utilities ===

Bills:
  [1] Electricity: 150
  [2] Internet: 80
  [3] Water: 45
```

#### All Tags
List all unique tags in the system:
```bash
npm start query tags
```

#### Active Goals
Show all active savings goals:
```bash
npm start query goals
```

## Database Schema

### Tables

#### `savings_goals`
```sql
CREATE TABLE savings_goals (
  id INTEGER PRIMARY KEY,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  target_amount TEXT NOT NULL,
  current_amount TEXT NOT NULL,
  target_date INTEGER NOT NULL,
  locked INTEGER NOT NULL,
  unlock_date INTEGER,
  tags TEXT NOT NULL DEFAULT '[]',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
```

#### `bills`
```sql
CREATE TABLE bills (
  id INTEGER PRIMARY KEY,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  amount TEXT NOT NULL,
  due_date INTEGER NOT NULL,
  recurring INTEGER NOT NULL,
  frequency_days INTEGER NOT NULL,
  paid INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  paid_at INTEGER,
  schedule_id INTEGER,
  tags TEXT NOT NULL DEFAULT '[]',
  updated_at INTEGER NOT NULL
);
```

#### `insurance_policies`
```sql
CREATE TABLE insurance_policies (
  id INTEGER PRIMARY KEY,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  coverage_type TEXT NOT NULL,
  monthly_premium TEXT NOT NULL,
  coverage_amount TEXT NOT NULL,
  active INTEGER NOT NULL,
  next_payment_date INTEGER NOT NULL,
  schedule_id INTEGER,
  tags TEXT NOT NULL DEFAULT '[]',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
```

#### `remittance_splits`
```sql
CREATE TABLE remittance_splits (
  id INTEGER PRIMARY KEY,
  owner TEXT NOT NULL,
  name TEXT NOT NULL,
  total_amount TEXT NOT NULL,
  recipients TEXT NOT NULL,
  executed INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  executed_at INTEGER,
  updated_at INTEGER NOT NULL
);
```

#### `events`
Raw event storage for audit and debugging:
```sql
CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ledger INTEGER NOT NULL,
  tx_hash TEXT NOT NULL,
  contract_address TEXT NOT NULL,
  event_type TEXT NOT NULL,
  topic TEXT NOT NULL,
  data TEXT NOT NULL,
  timestamp INTEGER NOT NULL
);
```

## Event Processing

### Supported Events

| Event Type | Contract | Action |
|------------|----------|--------|
| `goal_created` | Savings Goals | Create new goal record |
| `goal_deposit` | Savings Goals | Update current_amount |
| `goal_withdraw` | Savings Goals | Update current_amount |
| `bill_created` | Bill Payments | Create new bill record |
| `bill_paid` | Bill Payments | Mark bill as paid |
| `policy_created` | Insurance | Create new policy record |
| `split_created` | Remittance Split | Create new split record |
| `split_executed` | Remittance Split | Mark split as executed |
| `tags_add` | All Contracts | Add tags to entity |
| `tags_rem` | All Contracts | Remove tags from entity |

### Event Flow

1. **Poll**: Indexer polls Stellar RPC for new ledgers
2. **Fetch**: Retrieves events from monitored contracts
3. **Parse**: Converts Soroban ScVal format to JavaScript types
4. **Store**: Saves raw event to `events` table
5. **Process**: Updates normalized entity tables
6. **Checkpoint**: Records last processed ledger

## Development

### Project Structure

```
indexer/
├── src/
│   ├── db/
│   │   ├── schema.ts      # Database schema and initialization
│   │   └── queries.ts     # Query service with example queries
│   ├── types.ts           # TypeScript type definitions
│   ├── eventProcessor.ts  # Event parsing and processing logic
│   ├── indexer.ts         # Main indexer loop
│   ├── api.ts             # Query API service
│   └── index.ts           # Entry point
├── package.json
├── tsconfig.json
├── .env.example
└── README.md
```

### Adding New Event Types

1. Add event type to `eventProcessor.ts`:
```typescript
case 'new_event_type':
  this.processNewEvent(data, timestamp);
  break;
```

2. Implement processing function:
```typescript
private processNewEvent(data: any, timestamp: number): void {
  // Parse event data
  // Update database
}
```

3. Add query methods to `queries.ts` if needed

### Running in Development

Use `ts-node` for development without building:
```bash
npm run dev
```

## Testing Against Localnet

1. **Start Stellar localnet**:
```bash
stellar network start local
```

2. **Deploy contracts to localnet**:
```bash
cd ../
./scripts/deploy_local.sh
```

3. **Update `.env`** with localnet configuration:
```env
STELLAR_RPC_URL=http://localhost:8000/soroban/rpc
NETWORK_PASSPHRASE=Standalone Network ; February 2017
START_LEDGER=1
```

4. **Run indexer**:
```bash
npm start
```

5. **Generate test events** by interacting with contracts:
```bash
# Create a savings goal
stellar contract invoke \
  --id $SAVINGS_GOALS_CONTRACT \
  --source alice \
  -- create_goal \
  --caller alice \
  --name "Test Goal" \
  --target_amount 10000 \
  --target_date 1735689600

# Query indexed data
npm start query dashboard GXXXXXXX...
```

## Testing Against Testnet

1. **Deploy contracts to testnet**:
```bash
./scripts/deploy_testnet.sh
```

2. **Update `.env`** with testnet configuration:
```env
STELLAR_RPC_URL=https://soroban-testnet.stellar.org
NETWORK_PASSPHRASE=Test SDF Network ; September 2015
```

3. **Run indexer**:
```bash
npm start
```

## Performance Considerations

- **Poll Interval**: Default 5 seconds. Adjust based on network activity
- **Batch Processing**: Processes all events in a ledger range atomically
- **Database**: SQLite with WAL mode for better concurrency
- **Indexes**: Created on frequently queried columns (owner, dates, status)

## Limitations

- **No Real-time Updates**: Polling-based, not push-based
- **Single Instance**: Not designed for horizontal scaling
- **No Event Replay**: Reprocessing requires database reset
- **Basic Error Handling**: Retries on next poll cycle

## Future Enhancements

- [ ] HTTP REST API server
- [ ] GraphQL endpoint
- [ ] WebSocket support for real-time updates
- [ ] Event replay functionality
- [ ] Multi-instance coordination
- [ ] Prometheus metrics
- [ ] Advanced filtering and pagination
- [ ] Event subscription webhooks

## Troubleshooting

### Indexer not finding events

- Verify contract addresses in `.env`
- Check `START_LEDGER` is before contract deployment
- Ensure RPC endpoint is accessible
- Verify network passphrase matches network

### Database locked errors

- Only run one indexer instance per database
- Check file permissions on `data/` directory
- Ensure WAL mode is enabled

### Missing events

- Check `indexer_state` table for last processed ledger
- Verify events were emitted by contracts
- Review `events` table for raw event data

## License

MIT

## Support

For issues and questions:
- GitHub Issues: [Remitwise-Contracts/issues](https://github.com/your-org/Remitwise-Contracts/issues)
- Documentation: [ARCHITECTURE.md](../ARCHITECTURE.md)
