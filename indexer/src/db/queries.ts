/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import Database from 'better-sqlite3';
import { SavingsGoal, Bill, InsurancePolicy, RemittanceSplit } from '../types';

export class QueryService {
  constructor(private db: Database.Database) {}

  // Savings Goals queries
  getGoalsByOwner(owner: string): SavingsGoal[] {
    const stmt = this.db.prepare(`
      SELECT * FROM savings_goals 
      WHERE owner = ? 
      ORDER BY created_at DESC
    `);
    return stmt.all(owner) as SavingsGoal[];
  }

  getGoalById(id: number): SavingsGoal | undefined {
    const stmt = this.db.prepare('SELECT * FROM savings_goals WHERE id = ?');
    return stmt.get(id) as SavingsGoal | undefined;
  }

  getGoalsByTag(tag: string): SavingsGoal[] {
    const stmt = this.db.prepare(`
      SELECT * FROM savings_goals 
      WHERE tags LIKE ? 
      ORDER BY created_at DESC
    `);
    return stmt.all(`%"${tag}"%`) as SavingsGoal[];
  }

  getActiveGoals(): SavingsGoal[] {
    const now = Math.floor(Date.now() / 1000);
    const stmt = this.db.prepare(`
      SELECT * FROM savings_goals 
      WHERE target_date > ? AND (locked = 0 OR unlock_date < ?)
      ORDER BY target_date ASC
    `);
    return stmt.all(now, now) as SavingsGoal[];
  }

  // Bills queries
  getBillsByOwner(owner: string): Bill[] {
    const stmt = this.db.prepare(`
      SELECT * FROM bills 
      WHERE owner = ? 
      ORDER BY due_date ASC
    `);
    return stmt.all(owner) as Bill[];
  }

  getUnpaidBills(owner: string): Bill[] {
    const stmt = this.db.prepare(`
      SELECT * FROM bills 
      WHERE owner = ? AND paid = 0 
      ORDER BY due_date ASC
    `);
    return stmt.all(owner) as Bill[];
  }

  getOverdueBills(): Bill[] {
    const now = Math.floor(Date.now() / 1000);
    const stmt = this.db.prepare(`
      SELECT * FROM bills 
      WHERE paid = 0 AND due_date < ? 
      ORDER BY due_date ASC
    `);
    return stmt.all(now) as Bill[];
  }

  getBillsByTag(tag: string): Bill[] {
    const stmt = this.db.prepare(`
      SELECT * FROM bills 
      WHERE tags LIKE ? 
      ORDER BY due_date ASC
    `);
    return stmt.all(`%"${tag}"%`) as Bill[];
  }

  // Insurance Policies queries
  getPoliciesByOwner(owner: string): InsurancePolicy[] {
    const stmt = this.db.prepare(`
      SELECT * FROM insurance_policies 
      WHERE owner = ? 
      ORDER BY created_at DESC
    `);
    return stmt.all(owner) as InsurancePolicy[];
  }

  getActivePolicies(owner: string): InsurancePolicy[] {
    const stmt = this.db.prepare(`
      SELECT * FROM insurance_policies 
      WHERE owner = ? AND active = 1 
      ORDER BY next_payment_date ASC
    `);
    return stmt.all(owner) as InsurancePolicy[];
  }

  getPoliciesByTag(tag: string): InsurancePolicy[] {
    const stmt = this.db.prepare(`
      SELECT * FROM insurance_policies 
      WHERE tags LIKE ? 
      ORDER BY created_at DESC
    `);
    return stmt.all(`%"${tag}"%`) as InsurancePolicy[];
  }

  // Remittance Splits queries
  getSplitsByOwner(owner: string): RemittanceSplit[] {
    const stmt = this.db.prepare(`
      SELECT * FROM remittance_splits 
      WHERE owner = ? 
      ORDER BY created_at DESC
    `);
    return stmt.all(owner) as RemittanceSplit[];
  }

  getPendingSplits(owner: string): RemittanceSplit[] {
    const stmt = this.db.prepare(`
      SELECT * FROM remittance_splits 
      WHERE owner = ? AND executed = 0 
      ORDER BY created_at DESC
    `);
    return stmt.all(owner) as RemittanceSplit[];
  }

  // Analytics queries
  getTotalsByOwner(owner: string): {
    total_goals: number;
    total_savings: string;
    unpaid_bills: number;
    total_bills_amount: string;
    active_policies: number;
    total_coverage: string;
  } {
    const goals = this.db.prepare('SELECT COUNT(*) as count, SUM(CAST(current_amount AS REAL)) as total FROM savings_goals WHERE owner = ?').get(owner) as any;
    const bills = this.db.prepare('SELECT COUNT(*) as count, SUM(CAST(amount AS REAL)) as total FROM bills WHERE owner = ? AND paid = 0').get(owner) as any;
    const policies = this.db.prepare('SELECT COUNT(*) as count, SUM(CAST(coverage_amount AS REAL)) as total FROM insurance_policies WHERE owner = ? AND active = 1').get(owner) as any;

    return {
      total_goals: goals.count || 0,
      total_savings: (goals.total || 0).toString(),
      unpaid_bills: bills.count || 0,
      total_bills_amount: (bills.total || 0).toString(),
      active_policies: policies.count || 0,
      total_coverage: (policies.total || 0).toString(),
    };
  }

  // Get all unique tags
  getAllTags(): string[] {
    const tags = new Set<string>();
    
    const addTags = (rows: any[]) => {
      rows.forEach(row => {
        try {
          const tagArray = JSON.parse(row.tags);
          tagArray.forEach((tag: string) => tags.add(tag));
        } catch (e) {
          // Skip invalid JSON
        }
      });
    };

    addTags(this.db.prepare('SELECT tags FROM savings_goals').all());
    addTags(this.db.prepare('SELECT tags FROM bills').all());
    addTags(this.db.prepare('SELECT tags FROM insurance_policies').all());

    return Array.from(tags).sort();
  }
}
