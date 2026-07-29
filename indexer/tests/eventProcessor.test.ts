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

import { EventProcessor } from "../src/eventProcessor";
import { initializeDatabase } from "../src/db/schema";
import Database from "better-sqlite3";

describe("EventProcessor", () => {
  let db: Database.Database;
  let processor: EventProcessor;

  beforeEach(() => {
    // Create in-memory database for testing
    db = new Database(":memory:");
    db.pragma("journal_mode = WAL");

    // Initialize schema
    const { initializeDatabase: init } = require("../src/db/schema");
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

      CREATE TABLE family_wallet_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        event_type TEXT NOT NULL,
        contract_address TEXT NOT NULL,
        member TEXT,
        role TEXT,
        spending_limit TEXT,
        limit_amount TEXT,
        tx_id TEXT,
        proposer TEXT,
        recipient TEXT,
        amount TEXT,
        timestamp INTEGER NOT NULL,
        ledger INTEGER NOT NULL,
        tx_hash TEXT NOT NULL,
        raw_data TEXT NOT NULL
      );

      CREATE TABLE orchestrator_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        event_type TEXT NOT NULL,
        contract_address TEXT NOT NULL,
        executor TEXT,
        amount TEXT,
        error_code INTEGER,
        timestamp INTEGER NOT NULL,
        ledger INTEGER NOT NULL,
        tx_hash TEXT NOT NULL,
        raw_data TEXT NOT NULL
      );

      CREATE TABLE killswitch_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        event_type TEXT NOT NULL,
        contract_address TEXT NOT NULL,
        scope TEXT,
        module_id TEXT,
        func_name TEXT,
        timestamp INTEGER NOT NULL,
        ledger INTEGER NOT NULL,
        tx_hash TEXT NOT NULL,
        raw_data TEXT NOT NULL,
        alert_sent INTEGER NOT NULL DEFAULT 0
      );
    `);

    processor = new EventProcessor(db);
  });

  afterEach(() => {
    db.close();
  });

  describe("Goal Events", () => {
    test("should process goal_created event", () => {
      const mockEvent = {
        topic: ["savings", "goal_created"],
        body: {
          v0: {
            data: {
              goal_id: 1,
              owner: "GXXXXXXX",
              name: "Emergency Fund",
              target_amount: "10000",
              target_date: 1735689600,
            },
          },
        },
      };

      processor.processEvent(
        1000,
        "tx123",
        "contract123",
        mockEvent,
        1700000000,
      );

      const goal = db
        .prepare("SELECT * FROM savings_goals WHERE id = ?")
        .get(1);
      expect(goal).toBeDefined();
      expect(goal.name).toBe("Emergency Fund");
    });

    test("should process goal_deposit event", () => {
      // First create a goal
      db.prepare(
        `
        INSERT INTO savings_goals 
        (id, owner, name, target_amount, current_amount, target_date, locked, tags, created_at, updated_at)
        VALUES (1, 'GXXXXXXX', 'Test Goal', '10000', '0', 1735689600, 0, '[]', 1700000000, 1700000000)
      `,
      ).run();

      const mockEvent = {
        topic: ["savings", "goal_deposit"],
        body: {
          v0: {
            data: {
              goal_id: 1,
              amount: "1000",
            },
          },
        },
      };

      processor.processEvent(
        1001,
        "tx124",
        "contract123",
        mockEvent,
        1700000100,
      );

      const goal = db
        .prepare("SELECT * FROM savings_goals WHERE id = ?")
        .get(1);
      expect(parseFloat(goal.current_amount)).toBe(1000);
    });
  });

  describe("Bill Events", () => {
    test("should process bill_created event", () => {
      const mockEvent = {
        topic: ["bills", "bill_created"],
        body: {
          v0: {
            data: {
              bill_id: 1,
              owner: "GXXXXXXX",
              name: "Electricity",
              amount: "150",
              due_date: 1735689600,
              recurring: true,
            },
          },
        },
      };

      processor.processEvent(
        1000,
        "tx123",
        "contract456",
        mockEvent,
        1700000000,
      );

      const bill = db.prepare("SELECT * FROM bills WHERE id = ?").get(1);
      expect(bill).toBeDefined();
      expect(bill.name).toBe("Electricity");
      expect(bill.paid).toBe(0);
    });

    test("should process bill_paid event", () => {
      // First create a bill
      db.prepare(
        `
        INSERT INTO bills 
        (id, owner, name, amount, due_date, recurring, frequency_days, paid, created_at, tags, updated_at)
        VALUES (1, 'GXXXXXXX', 'Test Bill', '100', 1735689600, 0, 0, 0, 1700000000, '[]', 1700000000)
      `,
      ).run();

      const mockEvent = {
        topic: ["bills", "bill_paid"],
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
        "tx124",
        "contract456",
        mockEvent,
        1700000100,
      );

      const bill = db.prepare("SELECT * FROM bills WHERE id = ?").get(1);
      expect(bill.paid).toBe(1);
      expect(bill.paid_at).toBe(1700000100);
    });
  });

  describe("Raw Event Storage", () => {
    test("should store raw events", () => {
      const mockEvent = {
        topic: ["test", "event"],
        body: {
          v0: {
            data: { test: "data" },
          },
        },
      };

      processor.processEvent(
        1000,
        "tx123",
        "contract123",
        mockEvent,
        1700000000,
      );

      const events = db.prepare("SELECT * FROM events").all();
      expect(events.length).toBeGreaterThan(0);
      expect(events[0].ledger).toBe(1000);
      expect(events[0].tx_hash).toBe("tx123");
    });
  });

  // ---------------------------------------------------------------------------
  // Family Wallet Events
  // ---------------------------------------------------------------------------

  describe("Family Wallet Events", () => {
    test("should process member event (member added)", () => {
      const mockEvent = {
        topic: ["family", "member"],
        body: {
          v0: {
            data: {
              member: "GXXXXXXX",
              role: 3,
              spending_limit: "1000000000",
            },
          },
        },
      };

      processor.processEvent(
        1000,
        "tx100",
        "fw_contract",
        mockEvent,
        1700000000,
      );

      const row = db
        .prepare(
          "SELECT * FROM family_wallet_events WHERE event_type = 'member'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.member).toBe("GXXXXXXX");
      expect(row.role).toBe("3");
      expect(row.contract_address).toBe("fw_contract");
      expect(row.ledger).toBe(1000);
      expect(row.tx_hash).toBe("tx100");
    });

    test("should process limit event (spending limit updated)", () => {
      const mockEvent = {
        topic: ["family", "limit"],
        body: {
          v0: {
            data: {
              member: "GXXXXXXX",
              new_limit: "2000000000",
            },
          },
        },
      };

      processor.processEvent(
        1001,
        "tx101",
        "fw_contract",
        mockEvent,
        1700000100,
      );

      const row = db
        .prepare(
          "SELECT * FROM family_wallet_events WHERE event_type = 'limit'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.member).toBe("GXXXXXXX");
      expect(row.limit_amount).toBe("2000000000");
    });

    test("should process em_prop event (emergency proposal)", () => {
      const mockEvent = {
        topic: ["family", "em_prop"],
        body: {
          v0: {
            data: {
              proposer: "GPROPOSER",
              recipient: "GRECIPIENT",
              amount: "5000000000",
            },
          },
        },
      };

      processor.processEvent(
        1002,
        "tx102",
        "fw_contract",
        mockEvent,
        1700000200,
      );

      const row = db
        .prepare(
          "SELECT * FROM family_wallet_events WHERE event_type = 'em_prop'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.proposer).toBe("GPROPOSER");
      expect(row.recipient).toBe("GRECIPIENT");
      expect(row.amount).toBe("5000000000");
    });

    test("should process archived event (transaction archived)", () => {
      const mockEvent = {
        topic: ["family", "archived"],
        body: {
          v0: {
            data: {
              tx_id: 42,
            },
          },
        },
      };

      processor.processEvent(
        1003,
        "tx103",
        "fw_contract",
        mockEvent,
        1700000300,
      );

      const row = db
        .prepare(
          "SELECT * FROM family_wallet_events WHERE event_type = 'archived'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.tx_id).toBe("42");
    });

    test("should store multiple family wallet event types independently", () => {
      const fixtures = [
        { topic: ["family", "member"], data: { member: "G1", role: 3 } },
        {
          topic: ["family", "limit"],
          data: { member: "G1", new_limit: "500" },
        },
        {
          topic: ["family", "em_prop"],
          data: { proposer: "G1", recipient: "G2", amount: "100" },
        },
        { topic: ["family", "archived"], data: { tx_id: 1 } },
      ];

      fixtures.forEach((e, i) => {
        processor.processEvent(
          2000 + i,
          `tx2${i}`,
          "fw_contract",
          {
            topic: e.topic,
            body: { v0: { data: e.data } },
          },
          1700000000 + i,
        );
      });

      const rows = db.prepare("SELECT * FROM family_wallet_events").all();
      expect(rows.length).toBe(4);
    });
  });

  // ---------------------------------------------------------------------------
  // Orchestrator Events
  // ---------------------------------------------------------------------------

  describe("Orchestrator Events", () => {
    test("should process flow_ok event", () => {
      const mockEvent = {
        topic: ["orch", "flow_ok"],
        body: {
          v0: {
            data: ["GEXECUTOR", "10000000000"],
          },
        },
      };

      processor.processEvent(
        3000,
        "tx300",
        "orch_contract",
        mockEvent,
        1700001000,
      );

      const row = db
        .prepare(
          "SELECT * FROM orchestrator_events WHERE event_type = 'flow_ok'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.executor).toBe("GEXECUTOR");
      expect(row.amount).toBe("10000000000");
      expect(row.contract_address).toBe("orch_contract");
      expect(row.ledger).toBe(3000);
    });

    test("should process flow_fail event", () => {
      const mockEvent = {
        topic: ["orch", "flow_fail"],
        body: {
          v0: {
            data: ["GEXECUTOR", 2],
          },
        },
      };

      processor.processEvent(
        3001,
        "tx301",
        "orch_contract",
        mockEvent,
        1700001100,
      );

      const row = db
        .prepare(
          "SELECT * FROM orchestrator_events WHERE event_type = 'flow_fail'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.executor).toBe("GEXECUTOR");
      expect(row.error_code).toBe(2);
    });

    test("should process flow_ok with object-style data", () => {
      const mockEvent = {
        topic: ["orch", "flow_ok"],
        body: {
          v0: {
            data: { executor: "GEXEC2", amount: "500" },
          },
        },
      };

      processor.processEvent(
        3002,
        "tx302",
        "orch_contract",
        mockEvent,
        1700001200,
      );

      const row = db
        .prepare("SELECT * FROM orchestrator_events WHERE ledger = 3002")
        .get() as any;
      expect(row).toBeDefined();
      expect(row.executor).toBe("GEXEC2");
      expect(row.amount).toBe("500");
    });

    test("should store both flow_ok and flow_fail events", () => {
      processor.processEvent(
        3010,
        "tx310",
        "orch_contract",
        {
          topic: ["orch", "flow_ok"],
          body: { v0: { data: ["GEXEC", "100"] } },
        },
        1700002000,
      );

      processor.processEvent(
        3011,
        "tx311",
        "orch_contract",
        {
          topic: ["orch", "flow_fail"],
          body: { v0: { data: ["GEXEC", 3] } },
        },
        1700002100,
      );

      const rows = db
        .prepare("SELECT * FROM orchestrator_events WHERE ledger >= 3010")
        .all();
      expect(rows.length).toBe(2);
    });
  });

  // ---------------------------------------------------------------------------
  // Emergency Killswitch Events
  // ---------------------------------------------------------------------------

  describe("Emergency Killswitch Events", () => {
    test("should process paused event and set alert_sent = 1", () => {
      const mockEvent = {
        topic: ["emergency", "paused"],
        body: {
          v0: {
            data: ["GLOBAL", 1700000000],
          },
        },
      };

      processor.processEvent(
        4000,
        "tx400",
        "ks_contract",
        mockEvent,
        1700000000,
      );

      const row = db
        .prepare("SELECT * FROM killswitch_events WHERE event_type = 'paused'")
        .get() as any;
      expect(row).toBeDefined();
      expect(row.scope).toBe("GLOBAL");
      expect(row.contract_address).toBe("ks_contract");
      expect(row.ledger).toBe(4000);
      expect(row.alert_sent).toBe(1);
    });

    test("should process unpaused event with alert_sent = 0", () => {
      const mockEvent = {
        topic: ["emergency", "unpaused"],
        body: {
          v0: {
            data: ["GLOBAL", 1700000100],
          },
        },
      };

      processor.processEvent(
        4001,
        "tx401",
        "ks_contract",
        mockEvent,
        1700000100,
      );

      const row = db
        .prepare(
          "SELECT * FROM killswitch_events WHERE event_type = 'unpaused'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.scope).toBe("GLOBAL");
      expect(row.alert_sent).toBe(0);
    });

    test("should process f_paused event and set alert_sent = 1", () => {
      const mockEvent = {
        topic: ["emergency", "f_paused"],
        body: {
          v0: {
            data: ["remittance", "distribute", 1700000200],
          },
        },
      };

      processor.processEvent(
        4002,
        "tx402",
        "ks_contract",
        mockEvent,
        1700000200,
      );

      const row = db
        .prepare(
          "SELECT * FROM killswitch_events WHERE event_type = 'f_paused'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.module_id).toBe("remittance");
      expect(row.func_name).toBe("distribute");
      expect(row.alert_sent).toBe(1);
    });

    test("should process m_paused event and set alert_sent = 1", () => {
      const mockEvent = {
        topic: ["emergency", "m_paused"],
        body: {
          v0: {
            data: ["insurance", 1700000300],
          },
        },
      };

      processor.processEvent(
        4003,
        "tx403",
        "ks_contract",
        mockEvent,
        1700000300,
      );

      const row = db
        .prepare(
          "SELECT * FROM killswitch_events WHERE event_type = 'm_paused'",
        )
        .get() as any;
      expect(row).toBeDefined();
      expect(row.module_id).toBe("insurance");
      expect(row.func_name).toBeNull();
      expect(row.alert_sent).toBe(1);
    });

    test("should process f_paused with object-style data", () => {
      const mockEvent = {
        topic: ["emergency", "f_paused"],
        body: {
          v0: {
            data: { module_id: "savings", func_name: "withdraw" },
          },
        },
      };

      processor.processEvent(
        4004,
        "tx404",
        "ks_contract",
        mockEvent,
        1700000400,
      );

      const row = db
        .prepare("SELECT * FROM killswitch_events WHERE ledger = 4004")
        .get() as any;
      expect(row).toBeDefined();
      expect(row.module_id).toBe("savings");
      expect(row.func_name).toBe("withdraw");
    });

    test("should store all four killswitch event types", () => {
      const fixtures = [
        { topic: ["emergency", "paused"], data: ["GLOBAL", 0] },
        { topic: ["emergency", "unpaused"], data: ["GLOBAL", 0] },
        { topic: ["emergency", "f_paused"], data: ["mod", "fn", 0] },
        { topic: ["emergency", "m_paused"], data: ["mod", 0] },
      ];

      fixtures.forEach((f, i) => {
        processor.processEvent(
          5000 + i,
          `tx5${i}`,
          "ks_contract",
          {
            topic: f.topic,
            body: { v0: { data: f.data } },
          },
          1700010000 + i,
        );
      });

      const rows = db
        .prepare("SELECT * FROM killswitch_events WHERE ledger >= 5000")
        .all() as any[];
      expect(rows.length).toBe(4);

      const types = rows.map((r) => r.event_type);
      expect(types).toContain("paused");
      expect(types).toContain("unpaused");
      expect(types).toContain("f_paused");
      expect(types).toContain("m_paused");
    });

    test("paused, f_paused, m_paused events should all have alert_sent = 1", () => {
      const alertingFixtures = [
        { topic: ["emergency", "paused"], data: ["GLOBAL", 0] },
        { topic: ["emergency", "f_paused"], data: ["mod", "fn", 0] },
        { topic: ["emergency", "m_paused"], data: ["mod", 0] },
      ];

      alertingFixtures.forEach((f, i) => {
        processor.processEvent(
          6000 + i,
          `tx6${i}`,
          "ks_contract",
          {
            topic: f.topic,
            body: { v0: { data: f.data } },
          },
          1700020000 + i,
        );
      });

      const rows = db
        .prepare("SELECT * FROM killswitch_events WHERE ledger >= 6000")
        .all() as any[];
      expect(rows.length).toBe(3);
      rows.forEach((row) => {
        expect(row.alert_sent).toBe(1);
      });
    });
  });
});
