/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

/**
 * Unit tests for EventProcessor
 * Run with: npm test
 */

import { EventProcessor } from '../src/eventProcessor';
import { initializeDatabase } from '../src/db/schema';
import Database from 'better-sqlite3';

describe('EventProcessor', () => {
  let db: Database.Database;
  let processor: EventProcessor;

  beforeEach(() => {
    // Create in-memory database for testing
    db = new Database(':memory:');
    db.pragma('journal_mode = WAL');
    
    // Initialize schema
    const { initializeDatabase: init } = require('../src/db/schema');
    // Manually create tables for testing
    db.exec(`
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
    `);
    
    processor = new EventProcessor(db);
  });

  afterEach(() => {
    db.close();
  });

  describe('Goal Events', () => {
    test('should process goal_created event', () => {
      const mockEvent = {
        topic: ['savings', 'goal_created'],
        body: {
          v0: {
            data: {
              goal_id: 1,
              owner: 'GXXXXXXX',
              name: 'Emergency Fund',
              target_amount: '10000',
              target_date: 1735689600,
            },
          },
        },
      };

      processor.processEvent(
        1000,
        'tx123',
        'contract123',
        mockEvent,
        1700000000
      );

      const goal = db.prepare('SELECT * FROM savings_goals WHERE id = ?').get(1);
      expect(goal).toBeDefined();
      expect(goal.name).toBe('Emergency Fund');
    });

    test('should process goal_deposit event', () => {
      // First create a goal
      db.prepare(`
        INSERT INTO savings_goals 
        (id, owner, name, target_amount, current_amount, target_date, locked, tags, created_at, updated_at)
        VALUES (1, 'GXXXXXXX', 'Test Goal', '10000', '0', 1735689600, 0, '[]', 1700000000, 1700000000)
      `).run();

      const mockEvent = {
        topic: ['savings', 'goal_deposit'],
        body: {
          v0: {
            data: {
              goal_id: 1,
              amount: '1000',
            },
          },
        },
      };

      processor.processEvent(
        1001,
        'tx124',
        'contract123',
        mockEvent,
        1700000100
      );

      const goal = db.prepare('SELECT * FROM savings_goals WHERE id = ?').get(1);
      expect(parseFloat(goal.current_amount)).toBe(1000);
    });
  });

  describe('Bill Events', () => {
    test('should process bill_created event', () => {
      const mockEvent = {
        topic: ['bills', 'bill_created'],
        body: {
          v0: {
            data: {
              bill_id: 1,
              owner: 'GXXXXXXX',
              name: 'Electricity',
              amount: '150',
              due_date: 1735689600,
              recurring: true,
            },
          },
        },
      };

      processor.processEvent(
        1000,
        'tx123',
        'contract456',
        mockEvent,
        1700000000
      );

      const bill = db.prepare('SELECT * FROM bills WHERE id = ?').get(1);
      expect(bill).toBeDefined();
      expect(bill.name).toBe('Electricity');
      expect(bill.paid).toBe(0);
    });

    test('should process bill_paid event', () => {
      // First create a bill
      db.prepare(`
        INSERT INTO bills 
        (id, owner, name, amount, due_date, recurring, frequency_days, paid, created_at, tags, updated_at)
        VALUES (1, 'GXXXXXXX', 'Test Bill', '100', 1735689600, 0, 0, 0, 1700000000, '[]', 1700000000)
      `).run();

      const mockEvent = {
        topic: ['bills', 'bill_paid'],
        body: {
          v0: {
            data: {
              bill_id: 1,
            },
          },
        },
      };

      processor.processEvent(
        1001,
        'tx124',
        'contract456',
        mockEvent,
        1700000100
      );

      const bill = db.prepare('SELECT * FROM bills WHERE id = ?').get(1);
      expect(bill.paid).toBe(1);
      expect(bill.paid_at).toBe(1700000100);
    });
  });

  describe('Raw Event Storage', () => {
    test('should store raw events', () => {
      const mockEvent = {
        topic: ['test', 'event'],
        body: {
          v0: {
            data: { test: 'data' },
          },
        },
      };

      processor.processEvent(
        1000,
        'tx123',
        'contract123',
        mockEvent,
        1700000000
      );

      const events = db.prepare('SELECT * FROM events').all();
      expect(events.length).toBeGreaterThan(0);
      expect(events[0].ledger).toBe(1000);
      expect(events[0].tx_hash).toBe('tx123');
    });
  });
});
