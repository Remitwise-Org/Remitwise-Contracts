/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import Database from 'better-sqlite3';

export function initializeDatabase(dbPath: string): Database.Database {
  const db = new Database(dbPath);
  
  // Enable WAL mode for better concurrency
  db.pragma('journal_mode = WAL');
  
  createTables(db);
  
  return db;
}

function createTables(db: Database.Database): void {
  // Savings Goals table
  db.exec(`
    CREATE TABLE IF NOT EXISTS savings_goals (
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
    CREATE INDEX IF NOT EXISTS idx_goals_owner ON savings_goals(owner);
    CREATE INDEX IF NOT EXISTS idx_goals_target_date ON savings_goals(target_date);
  `);

  // Bills table
  db.exec(`
    CREATE TABLE IF NOT EXISTS bills (
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
    CREATE INDEX IF NOT EXISTS idx_bills_owner ON bills(owner);
    CREATE INDEX IF NOT EXISTS idx_bills_due_date ON bills(due_date);
    CREATE INDEX IF NOT EXISTS idx_bills_paid ON bills(paid);
  `);

  // Insurance Policies table
  db.exec(`
    CREATE TABLE IF NOT EXISTS insurance_policies (
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
    CREATE INDEX IF NOT EXISTS idx_policies_owner ON insurance_policies(owner);
    CREATE INDEX IF NOT EXISTS idx_policies_active ON insurance_policies(active);
  `);

  // Remittance Splits table
  db.exec(`
    CREATE TABLE IF NOT EXISTS remittance_splits (
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
    CREATE INDEX IF NOT EXISTS idx_splits_owner ON remittance_splits(owner);
    CREATE INDEX IF NOT EXISTS idx_splits_executed ON remittance_splits(executed);
  `);

  // Events table for raw event storage
  db.exec(`
    CREATE TABLE IF NOT EXISTS events (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      ledger INTEGER NOT NULL,
      tx_hash TEXT NOT NULL,
      contract_address TEXT NOT NULL,
      event_type TEXT NOT NULL,
      topic TEXT NOT NULL,
      data TEXT NOT NULL,
      timestamp INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_events_ledger ON events(ledger);
    CREATE INDEX IF NOT EXISTS idx_events_contract ON events(contract_address);
    CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
  `);

  // Indexer state table
  db.exec(`
    CREATE TABLE IF NOT EXISTS indexer_state (
      key TEXT PRIMARY KEY,
      value TEXT NOT NULL
    );
  `);

  console.log('Database schema initialized');
}
