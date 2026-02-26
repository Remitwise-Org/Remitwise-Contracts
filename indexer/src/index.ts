/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import dotenv from 'dotenv';
import { initializeDatabase } from './db/schema';
import { Indexer } from './indexer';
import { ApiService } from './api';
import * as fs from 'fs';
import * as path from 'path';

// Load environment variables
dotenv.config();

function validateEnv(): void {
  const required = [
    'STELLAR_RPC_URL',
    'BILL_PAYMENTS_CONTRACT',
    'SAVINGS_GOALS_CONTRACT',
    'INSURANCE_CONTRACT',
  ];

  const missing = required.filter(key => !process.env[key]);
  
  if (missing.length > 0) {
    console.error('Missing required environment variables:');
    missing.forEach(key => console.error(`  - ${key}`));
    console.error('\nPlease copy .env.example to .env and configure it.');
    process.exit(1);
  }
}

function ensureDataDirectory(): void {
  const dbPath = process.env.DB_PATH || './data/remitwise.db';
  const dataDir = path.dirname(dbPath);
  
  if (!fs.existsSync(dataDir)) {
    fs.mkdirSync(dataDir, { recursive: true });
    console.log(`Created data directory: ${dataDir}`);
  }
}

async function main() {
  console.log('Remitwise Indexer v1.0.0\n');

  // Validate environment
  validateEnv();
  ensureDataDirectory();

  // Initialize database
  const dbPath = process.env.DB_PATH || './data/remitwise.db';
  console.log(`Initializing database: ${dbPath}`);
  const db = initializeDatabase(dbPath);

  // Get contract addresses
  const contracts = [
    process.env.BILL_PAYMENTS_CONTRACT!,
    process.env.SAVINGS_GOALS_CONTRACT!,
    process.env.INSURANCE_CONTRACT!,
  ];

  if (process.env.REMITTANCE_SPLIT_CONTRACT) {
    contracts.push(process.env.REMITTANCE_SPLIT_CONTRACT);
  }

  // Parse command line arguments
  const args = process.argv.slice(2);
  const command = args[0];

  if (command === 'query') {
    // Query mode - run example queries
    await runQueryExamples(db, args.slice(1));
  } else {
    // Indexer mode - start indexing
    const pollInterval = parseInt(process.env.POLL_INTERVAL_MS || '5000');
    const indexer = new Indexer(
      db,
      process.env.STELLAR_RPC_URL!,
      contracts,
      pollInterval
    );

    // Handle graceful shutdown
    process.on('SIGINT', () => {
      console.log('\nReceived SIGINT, shutting down gracefully...');
      indexer.stop();
      db.close();
      process.exit(0);
    });

    process.on('SIGTERM', () => {
      console.log('\nReceived SIGTERM, shutting down gracefully...');
      indexer.stop();
      db.close();
      process.exit(0);
    });

    // Start indexing
    await indexer.start();
  }
}

async function runQueryExamples(db: any, args: string[]): Promise<void> {
  const api = new ApiService(db);
  const queryType = args[0];
  const param = args[1];

  console.log('Running query examples...\n');

  switch (queryType) {
    case 'dashboard':
      if (!param) {
        console.error('Usage: npm start query dashboard <owner_address>');
        process.exit(1);
      }
      api.printUserDashboard(param);
      break;

    case 'overdue':
      api.printOverdueBills();
      break;

    case 'tag':
      if (!param) {
        console.error('Usage: npm start query tag <tag_name>');
        process.exit(1);
      }
      api.printEntitiesByTag(param);
      break;

    case 'tags':
      api.printAllTags();
      break;

    case 'goals':
      const goals = api.getActiveGoals();
      console.log('=== Active Goals ===');
      goals.forEach(goal => {
        const tags = JSON.parse(goal.tags);
        console.log(`[${goal.id}] ${goal.name}: ${goal.current_amount}/${goal.target_amount} ${tags.length > 0 ? `[${tags.join(', ')}]` : ''}`);
      });
      console.log('');
      break;

    default:
      console.log('Available query commands:');
      console.log('  dashboard <owner_address> - Show user dashboard');
      console.log('  overdue                   - Show all overdue bills');
      console.log('  tag <tag_name>            - Show entities with specific tag');
      console.log('  tags                      - Show all tags');
      console.log('  goals                     - Show active goals');
      console.log('');
      console.log('Example:');
      console.log('  npm start query dashboard GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX');
      break;
  }

  db.close();
}

// Run the application
main().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
