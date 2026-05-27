/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import Database from "better-sqlite3";
import { xdr } from "@stellar/stellar-sdk";

export class EventProcessor {
  constructor(private db: Database.Database) {}

  processEvent(
    ledger: number,
    txHash: string,
    contractAddress: string,
    event: any,
    timestamp: number,
  ): void {
    try {
      const topic = this.parseEventTopic(event);
      const data = this.parseEventData(event);

      // Store raw event
      this.storeRawEvent(
        ledger,
        txHash,
        contractAddress,
        topic,
        data,
        timestamp,
      );

      // Process specific event types
      this.processSpecificEvent(
        contractAddress,
        topic,
        data,
        timestamp,
        ledger,
        txHash,
      );
    } catch (error) {
      console.error("Error processing event:", error);
    }
  }

  private parseEventTopic(event: any): string {
    // Extract event topic from Soroban event structure
    if (event.topic && Array.isArray(event.topic)) {
      return event.topic.map((t: any) => this.scValToString(t)).join("::");
    }
    return "unknown";
  }

  private parseEventData(event: any): any {
    // Parse event data from ScVal format
    if (event.body && event.body.v0 && event.body.v0.data) {
      return this.scValToJs(event.body.v0.data);
    }
    return {};
  }

  private scValToString(scVal: any): string {
    // Convert ScVal to string representation
    if (scVal.sym) return scVal.sym.toString();
    if (scVal.u32) return scVal.u32.toString();
    if (scVal.i32) return scVal.i32.toString();
    if (scVal.str) return scVal.str.toString();
    return JSON.stringify(scVal);
  }

  private scValToJs(scVal: any): any {
    // Convert ScVal to JavaScript types
    if (scVal.u32 !== undefined) return scVal.u32;
    if (scVal.i32 !== undefined) return scVal.i32;
    if (scVal.u64 !== undefined) return scVal.u64.toString();
    if (scVal.i64 !== undefined) return scVal.i64.toString();
    if (scVal.i128 !== undefined) return scVal.i128.toString();
    if (scVal.str !== undefined) return scVal.str.toString();
    if (scVal.sym !== undefined) return scVal.sym.toString();
    if (scVal.bool !== undefined) return scVal.bool;
    if (scVal.address !== undefined) return scVal.address.toString();
    if (scVal.vec !== undefined) {
      return scVal.vec.map((v: any) => this.scValToJs(v));
    }
    if (scVal.map !== undefined) {
      const obj: any = {};
      scVal.map.forEach((entry: any) => {
        const key = this.scValToJs(entry.key);
        const val = this.scValToJs(entry.val);
        obj[key] = val;
      });
      return obj;
    }
    return scVal;
  }

  private storeRawEvent(
    ledger: number,
    txHash: string,
    contractAddress: string,
    topic: string,
    data: any,
    timestamp: number,
  ): void {
    const stmt = this.db.prepare(`
      INSERT INTO events (ledger, tx_hash, contract_address, event_type, topic, data, timestamp)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `);

    stmt.run(
      ledger,
      txHash,
      contractAddress,
      this.extractEventType(topic),
      topic,
      JSON.stringify(data),
      timestamp,
    );
  }

  private extractEventType(topic: string): string {
    // Extract event type from topic
    const parts = topic.split("::");
    return parts[parts.length - 1] || "unknown";
  }

  private processSpecificEvent(
    contractAddress: string,
    topic: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    const eventType = this.extractEventType(topic);

    // Process based on event type
    switch (eventType) {
      case "goal_created":
        this.processGoalCreated(data, timestamp);
        break;
      case "goal_deposit":
        this.processGoalDeposit(data, timestamp);
        break;
      case "goal_withdraw":
        this.processGoalWithdraw(data, timestamp);
        break;
      case "bill_created":
        this.processBillCreated(data, timestamp);
        break;
      case "bill_paid":
        this.processBillPaid(data, timestamp);
        break;
      case "policy_created":
        this.processPolicyCreated(data, timestamp);
        break;
      case "split_created":
        this.processSplitCreated(data, timestamp);
        break;
      case "split_executed":
        this.processSplitExecuted(data, timestamp);
        break;
      case "tags_add":
        this.processTagsAdded(contractAddress, data, timestamp);
        break;
      case "tags_rem":
        this.processTagsRemoved(contractAddress, data, timestamp);
        break;
      // Family Wallet events
      case "member":
        this.processFamilyWalletMember(
          contractAddress,
          eventType,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      case "limit":
        this.processFamilyWalletLimit(
          contractAddress,
          eventType,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      case "em_prop":
        this.processFamilyWalletEmProp(
          contractAddress,
          eventType,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      case "archived":
        this.processFamilyWalletArchived(
          contractAddress,
          eventType,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      // Orchestrator events
      case "flow_ok":
        this.processOrchestratorFlowOk(
          contractAddress,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      case "flow_fail":
        this.processOrchestratorFlowFail(
          contractAddress,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      // Emergency Killswitch events — also trigger alerting
      case "paused":
        this.processKillswitchPaused(
          contractAddress,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      case "unpaused":
        this.processKillswitchUnpaused(
          contractAddress,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      case "f_paused":
        this.processKillswitchFunctionPaused(
          contractAddress,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      case "m_paused":
        this.processKillswitchModulePaused(
          contractAddress,
          data,
          timestamp,
          ledger,
          txHash,
        );
        break;
      default:
        // Unknown event type, already stored in raw events
        break;
    }
  }

  private processGoalCreated(data: any, timestamp: number): void {
    const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO savings_goals 
      (id, owner, name, target_amount, current_amount, target_date, locked, unlock_date, tags, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);

    stmt.run(
      data.goal_id || data[0],
      data.owner || data[1],
      data.name || data[2] || "Unnamed Goal",
      data.target_amount || data[3] || "0",
      "0",
      data.target_date || data[4] || 0,
      0,
      null,
      "[]",
      timestamp,
      timestamp,
    );
  }

  private processGoalDeposit(data: any, timestamp: number): void {
    const goalId = data.goal_id || data[0];
    const amount = data.amount || data[1];

    const stmt = this.db.prepare(`
      UPDATE savings_goals 
      SET current_amount = CAST((CAST(current_amount AS REAL) + ?) AS TEXT),
          updated_at = ?
      WHERE id = ?
    `);

    stmt.run(parseFloat(amount), timestamp, goalId);
  }

  private processGoalWithdraw(data: any, timestamp: number): void {
    const goalId = data.goal_id || data[0];
    const amount = data.amount || data[1];

    const stmt = this.db.prepare(`
      UPDATE savings_goals 
      SET current_amount = CAST((CAST(current_amount AS REAL) - ?) AS TEXT),
          updated_at = ?
      WHERE id = ?
    `);

    stmt.run(parseFloat(amount), timestamp, goalId);
  }

  private processBillCreated(data: any, timestamp: number): void {
    const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO bills 
      (id, owner, name, amount, due_date, recurring, frequency_days, paid, created_at, paid_at, schedule_id, tags, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);

    stmt.run(
      data.bill_id || data[0],
      data.owner || data[1],
      data.name || data[2] || "Unnamed Bill",
      data.amount || data[3] || "0",
      data.due_date || data[4] || 0,
      data.recurring || data[5] || 0,
      data.frequency_days || 0,
      0,
      timestamp,
      null,
      null,
      "[]",
      timestamp,
    );
  }

  private processBillPaid(data: any, timestamp: number): void {
    const billId = data.bill_id || data[0];

    const stmt = this.db.prepare(`
      UPDATE bills 
      SET paid = 1, paid_at = ?, updated_at = ?
      WHERE id = ?
    `);

    stmt.run(timestamp, timestamp, billId);
  }

  private processPolicyCreated(data: any, timestamp: number): void {
    const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO insurance_policies 
      (id, owner, name, coverage_type, monthly_premium, coverage_amount, active, next_payment_date, schedule_id, tags, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);

    stmt.run(
      data.policy_id || data[0],
      data.owner || data[1],
      data.name || data[2] || "Unnamed Policy",
      data.coverage_type || data[3] || "General",
      data.monthly_premium || data[4] || "0",
      data.coverage_amount || data[5] || "0",
      1,
      data.next_payment_date || 0,
      null,
      "[]",
      timestamp,
      timestamp,
    );
  }

  private processSplitCreated(data: any, timestamp: number): void {
    const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO remittance_splits 
      (id, owner, name, total_amount, recipients, executed, created_at, executed_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);

    stmt.run(
      data.split_id || data[0],
      data.owner || data[1],
      data.name || data[2] || "Unnamed Split",
      data.total_amount || data[3] || "0",
      JSON.stringify(data.recipients || []),
      0,
      timestamp,
      null,
      timestamp,
    );
  }

  private processSplitExecuted(data: any, timestamp: number): void {
    const splitId = data.split_id || data[0];

    const stmt = this.db.prepare(`
      UPDATE remittance_splits 
      SET executed = 1, executed_at = ?, updated_at = ?
      WHERE id = ?
    `);

    stmt.run(timestamp, timestamp, splitId);
  }

  private processTagsAdded(
    contractAddress: string,
    data: any,
    timestamp: number,
  ): void {
    const entityId = data.entity_id || data[0];
    const tags = data.tags || data[2] || [];

    const table = this.getTableForContract(contractAddress);
    if (!table) return;

    const current = this.db
      .prepare(`SELECT tags FROM ${table} WHERE id = ?`)
      .get(entityId) as any;
    if (!current) return;

    const currentTags = JSON.parse(current.tags || "[]");
    const updatedTags = [...currentTags, ...tags];

    const stmt = this.db.prepare(`
      UPDATE ${table} 
      SET tags = ?, updated_at = ?
      WHERE id = ?
    `);

    stmt.run(JSON.stringify(updatedTags), timestamp, entityId);
  }

  private processTagsRemoved(
    contractAddress: string,
    data: any,
    timestamp: number,
  ): void {
    const entityId = data.entity_id || data[0];
    const tagsToRemove = data.tags || data[2] || [];

    const table = this.getTableForContract(contractAddress);
    if (!table) return;

    const current = this.db
      .prepare(`SELECT tags FROM ${table} WHERE id = ?`)
      .get(entityId) as any;
    if (!current) return;

    const currentTags = JSON.parse(current.tags || "[]");
    const updatedTags = currentTags.filter(
      (tag: string) => !tagsToRemove.includes(tag),
    );

    const stmt = this.db.prepare(`
      UPDATE ${table} 
      SET tags = ?, updated_at = ?
      WHERE id = ?
    `);

    stmt.run(JSON.stringify(updatedTags), timestamp, entityId);
  }

  private getTableForContract(contractAddress: string): string | null {
    // Map contract addresses to table names
    // This should be configured based on your deployed contracts
    const billsContract = process.env.BILL_PAYMENTS_CONTRACT;
    const goalsContract = process.env.SAVINGS_GOALS_CONTRACT;
    const insuranceContract = process.env.INSURANCE_CONTRACT;

    if (contractAddress === billsContract) return "bills";
    if (contractAddress === goalsContract) return "savings_goals";
    if (contractAddress === insuranceContract) return "insurance_policies";

    return null;
  }

  // ---------------------------------------------------------------------------
  // Family Wallet event processors
  // Topics: member, limit, em_prop, archived
  // ---------------------------------------------------------------------------

  private processFamilyWalletMember(
    contractAddress: string,
    eventType: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    const stmt = this.db.prepare(`
      INSERT INTO family_wallet_events
        (event_type, contract_address, member, role, spending_limit, timestamp, ledger, tx_hash, raw_data)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
    stmt.run(
      eventType,
      contractAddress,
      data.member || data[0] || null,
      data.role !== undefined ? String(data.role) : null,
      data.spending_limit !== undefined ? String(data.spending_limit) : null,
      timestamp,
      ledger,
      txHash,
      JSON.stringify(data),
    );
  }

  private processFamilyWalletLimit(
    contractAddress: string,
    eventType: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    const stmt = this.db.prepare(`
      INSERT INTO family_wallet_events
        (event_type, contract_address, member, limit_amount, timestamp, ledger, tx_hash, raw_data)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `);
    stmt.run(
      eventType,
      contractAddress,
      data.member || data[0] || null,
      data.new_limit !== undefined
        ? String(data.new_limit)
        : data[1] !== undefined
          ? String(data[1])
          : null,
      timestamp,
      ledger,
      txHash,
      JSON.stringify(data),
    );
  }

  private processFamilyWalletEmProp(
    contractAddress: string,
    eventType: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    const stmt = this.db.prepare(`
      INSERT INTO family_wallet_events
        (event_type, contract_address, proposer, recipient, amount, timestamp, ledger, tx_hash, raw_data)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
    `);
    stmt.run(
      eventType,
      contractAddress,
      data.proposer || data[0] || null,
      data.recipient || data[1] || null,
      data.amount !== undefined
        ? String(data.amount)
        : data[2] !== undefined
          ? String(data[2])
          : null,
      timestamp,
      ledger,
      txHash,
      JSON.stringify(data),
    );
  }

  private processFamilyWalletArchived(
    contractAddress: string,
    eventType: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    const stmt = this.db.prepare(`
      INSERT INTO family_wallet_events
        (event_type, contract_address, tx_id, timestamp, ledger, tx_hash, raw_data)
      VALUES (?, ?, ?, ?, ?, ?, ?)
    `);
    stmt.run(
      eventType,
      contractAddress,
      data.tx_id !== undefined
        ? String(data.tx_id)
        : data[0] !== undefined
          ? String(data[0])
          : null,
      timestamp,
      ledger,
      txHash,
      JSON.stringify(data),
    );
  }

  // ---------------------------------------------------------------------------
  // Orchestrator event processors
  // Topics: flow_ok, flow_fail
  // ---------------------------------------------------------------------------

  private processOrchestratorFlowOk(
    contractAddress: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    const stmt = this.db.prepare(`
      INSERT INTO orchestrator_events
        (event_type, contract_address, executor, amount, timestamp, ledger, tx_hash, raw_data)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `);
    // flow_ok payload: (executor: Address, amount: i128)
    const executor =
      data[0] !== undefined ? String(data[0]) : data.executor || null;
    const amount =
      data[1] !== undefined
        ? String(data[1])
        : data.amount !== undefined
          ? String(data.amount)
          : null;
    stmt.run(
      "flow_ok",
      contractAddress,
      executor,
      amount,
      timestamp,
      ledger,
      txHash,
      JSON.stringify(data),
    );
  }

  private processOrchestratorFlowFail(
    contractAddress: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    const stmt = this.db.prepare(`
      INSERT INTO orchestrator_events
        (event_type, contract_address, executor, error_code, timestamp, ledger, tx_hash, raw_data)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?)
    `);
    // flow_fail payload: (executor: Address, error_code: u32)
    const executor =
      data[0] !== undefined ? String(data[0]) : data.executor || null;
    const errorCode =
      data[1] !== undefined
        ? Number(data[1])
        : data.error_code !== undefined
          ? Number(data.error_code)
          : null;
    stmt.run(
      "flow_fail",
      contractAddress,
      executor,
      errorCode,
      timestamp,
      ledger,
      txHash,
      JSON.stringify(data),
    );
  }

  // ---------------------------------------------------------------------------
  // Emergency Killswitch event processors
  // Topics: paused, unpaused, f_paused, m_paused
  // These events drive alerting hooks — alert_sent is set to 0 on insert
  // and updated to 1 by the alerting subsystem after notification is sent.
  // ---------------------------------------------------------------------------

  private processKillswitchPaused(
    contractAddress: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    // paused payload: (scope: Symbol, timestamp: u64)
    const scope =
      data[0] !== undefined ? String(data[0]) : data.scope || "GLOBAL";
    this.insertKillswitchEvent(
      "paused",
      contractAddress,
      scope,
      null,
      null,
      timestamp,
      ledger,
      txHash,
      data,
    );
    this.triggerKillswitchAlert(
      "paused",
      contractAddress,
      scope,
      null,
      null,
      timestamp,
    );
  }

  private processKillswitchUnpaused(
    contractAddress: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    // unpaused payload: (scope: Symbol, timestamp: u64)
    const scope =
      data[0] !== undefined ? String(data[0]) : data.scope || "GLOBAL";
    this.insertKillswitchEvent(
      "unpaused",
      contractAddress,
      scope,
      null,
      null,
      timestamp,
      ledger,
      txHash,
      data,
    );
  }

  private processKillswitchFunctionPaused(
    contractAddress: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    // f_paused payload: (module_id: Symbol, func: Symbol, timestamp: u64)
    const moduleId =
      data[0] !== undefined ? String(data[0]) : data.module_id || null;
    const funcName =
      data[1] !== undefined ? String(data[1]) : data.func_name || null;
    this.insertKillswitchEvent(
      "f_paused",
      contractAddress,
      null,
      moduleId,
      funcName,
      timestamp,
      ledger,
      txHash,
      data,
    );
    this.triggerKillswitchAlert(
      "f_paused",
      contractAddress,
      null,
      moduleId,
      funcName,
      timestamp,
    );
  }

  private processKillswitchModulePaused(
    contractAddress: string,
    data: any,
    timestamp: number,
    ledger: number,
    txHash: string,
  ): void {
    // m_paused payload: (module_id: Symbol, timestamp: u64)
    const moduleId =
      data[0] !== undefined ? String(data[0]) : data.module_id || null;
    this.insertKillswitchEvent(
      "m_paused",
      contractAddress,
      null,
      moduleId,
      null,
      timestamp,
      ledger,
      txHash,
      data,
    );
    this.triggerKillswitchAlert(
      "m_paused",
      contractAddress,
      null,
      moduleId,
      null,
      timestamp,
    );
  }

  private insertKillswitchEvent(
    eventType: string,
    contractAddress: string,
    scope: string | null,
    moduleId: string | null,
    funcName: string | null,
    timestamp: number,
    ledger: number,
    txHash: string,
    data: any,
  ): void {
    const stmt = this.db.prepare(`
      INSERT INTO killswitch_events
        (event_type, contract_address, scope, module_id, func_name, timestamp, ledger, tx_hash, raw_data, alert_sent)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
    `);
    stmt.run(
      eventType,
      contractAddress,
      scope,
      moduleId,
      funcName,
      timestamp,
      ledger,
      txHash,
      JSON.stringify(data),
    );
  }

  /**
   * Trigger an alerting hook for killswitch/emergency events.
   *
   * In production this would call an external alerting service (PagerDuty,
   * Slack webhook, etc.). The implementation here logs to stderr so that
   * process supervisors (systemd, Docker) can capture and route the output.
   * The `alert_sent` column is updated to 1 after the alert is dispatched.
   */
  private triggerKillswitchAlert(
    eventType: string,
    contractAddress: string,
    scope: string | null,
    moduleId: string | null,
    funcName: string | null,
    timestamp: number,
  ): void {
    const detail = funcName
      ? `module=${moduleId} func=${funcName}`
      : moduleId
        ? `module=${moduleId}`
        : `scope=${scope}`;

    console.error(
      `[ALERT] killswitch event: type=${eventType} contract=${contractAddress} ${detail} ts=${timestamp}`,
    );

    // Mark the most recently inserted row as alerted
    this.db
      .prepare(
        `UPDATE killswitch_events SET alert_sent = 1
         WHERE id = (SELECT MAX(id) FROM killswitch_events WHERE event_type = ? AND contract_address = ?)`,
      )
      .run(eventType, contractAddress);
  }
}
