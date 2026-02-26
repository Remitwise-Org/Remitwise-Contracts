/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import Database from 'better-sqlite3';
import { xdr } from '@stellar/stellar-sdk';

export class EventProcessor {
  constructor(private db: Database.Database) {}

  processEvent(
    ledger: number,
    txHash: string,
    contractAddress: string,
    event: any,
    timestamp: number
  ): void {
    try {
      const topic = this.parseEventTopic(event);
      const data = this.parseEventData(event);

      // Store raw event
      this.storeRawEvent(ledger, txHash, contractAddress, topic, data, timestamp);

      // Process specific event types
      this.processSpecificEvent(contractAddress, topic, data, timestamp);
    } catch (error) {
      console.error('Error processing event:', error);
    }
  }

  private parseEventTopic(event: any): string {
    // Extract event topic from Soroban event structure
    if (event.topic && Array.isArray(event.topic)) {
      return event.topic.map((t: any) => this.scValToString(t)).join('::');
    }
    return 'unknown';
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
    timestamp: number
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
      timestamp
    );
  }

  private extractEventType(topic: string): string {
    // Extract event type from topic
    const parts = topic.split('::');
    return parts[parts.length - 1] || 'unknown';
  }

  private processSpecificEvent(
    contractAddress: string,
    topic: string,
    data: any,
    timestamp: number
  ): void {
    const eventType = this.extractEventType(topic);

    // Process based on event type
    switch (eventType) {
      case 'goal_created':
        this.processGoalCreated(data, timestamp);
        break;
      case 'goal_deposit':
        this.processGoalDeposit(data, timestamp);
        break;
      case 'goal_withdraw':
        this.processGoalWithdraw(data, timestamp);
        break;
      case 'bill_created':
        this.processBillCreated(data, timestamp);
        break;
      case 'bill_paid':
        this.processBillPaid(data, timestamp);
        break;
      case 'policy_created':
        this.processPolicyCreated(data, timestamp);
        break;
      case 'split_created':
        this.processSplitCreated(data, timestamp);
        break;
      case 'split_executed':
        this.processSplitExecuted(data, timestamp);
        break;
      case 'tags_add':
        this.processTagsAdded(contractAddress, data, timestamp);
        break;
      case 'tags_rem':
        this.processTagsRemoved(contractAddress, data, timestamp);
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
      data.name || data[2] || 'Unnamed Goal',
      data.target_amount || data[3] || '0',
      '0',
      data.target_date || data[4] || 0,
      0,
      null,
      '[]',
      timestamp,
      timestamp
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
      data.name || data[2] || 'Unnamed Bill',
      data.amount || data[3] || '0',
      data.due_date || data[4] || 0,
      data.recurring || data[5] || 0,
      data.frequency_days || 0,
      0,
      timestamp,
      null,
      null,
      '[]',
      timestamp
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
      data.name || data[2] || 'Unnamed Policy',
      data.coverage_type || data[3] || 'General',
      data.monthly_premium || data[4] || '0',
      data.coverage_amount || data[5] || '0',
      1,
      data.next_payment_date || 0,
      null,
      '[]',
      timestamp,
      timestamp
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
      data.name || data[2] || 'Unnamed Split',
      data.total_amount || data[3] || '0',
      JSON.stringify(data.recipients || []),
      0,
      timestamp,
      null,
      timestamp
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

  private processTagsAdded(contractAddress: string, data: any, timestamp: number): void {
    const entityId = data.entity_id || data[0];
    const tags = data.tags || data[2] || [];
    
    const table = this.getTableForContract(contractAddress);
    if (!table) return;

    const current = this.db.prepare(`SELECT tags FROM ${table} WHERE id = ?`).get(entityId) as any;
    if (!current) return;

    const currentTags = JSON.parse(current.tags || '[]');
    const updatedTags = [...currentTags, ...tags];
    
    const stmt = this.db.prepare(`
      UPDATE ${table} 
      SET tags = ?, updated_at = ?
      WHERE id = ?
    `);
    
    stmt.run(JSON.stringify(updatedTags), timestamp, entityId);
  }

  private processTagsRemoved(contractAddress: string, data: any, timestamp: number): void {
    const entityId = data.entity_id || data[0];
    const tagsToRemove = data.tags || data[2] || [];
    
    const table = this.getTableForContract(contractAddress);
    if (!table) return;

    const current = this.db.prepare(`SELECT tags FROM ${table} WHERE id = ?`).get(entityId) as any;
    if (!current) return;

    const currentTags = JSON.parse(current.tags || '[]');
    const updatedTags = currentTags.filter((tag: string) => !tagsToRemove.includes(tag));
    
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

    if (contractAddress === billsContract) return 'bills';
    if (contractAddress === goalsContract) return 'savings_goals';
    if (contractAddress === insuranceContract) return 'insurance_policies';
    
    return null;
  }
}
