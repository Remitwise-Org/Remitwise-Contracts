/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import Database from 'better-sqlite3';
import { QueryService } from './db/queries';

/**
 * Simple API service for querying indexed data
 * In production, this would be exposed via HTTP/REST or GraphQL
 */
export class ApiService {
  private queries: QueryService;

  constructor(db: Database.Database) {
    this.queries = new QueryService(db);
  }

  // Example query methods that can be exposed via HTTP endpoints

  getUserDashboard(owner: string) {
    return {
      owner,
      savings_goals: this.queries.getGoalsByOwner(owner),
      unpaid_bills: this.queries.getUnpaidBills(owner),
      active_policies: this.queries.getActivePolicies(owner),
      pending_splits: this.queries.getPendingSplits(owner),
      totals: this.queries.getTotalsByOwner(owner),
    };
  }

  getGoalDetails(goalId: number) {
    return this.queries.getGoalById(goalId);
  }

  getOverdueBills() {
    return this.queries.getOverdueBills();
  }

  getEntitiesByTag(tag: string) {
    return {
      tag,
      goals: this.queries.getGoalsByTag(tag),
      bills: this.queries.getBillsByTag(tag),
      policies: this.queries.getPoliciesByTag(tag),
    };
  }

  getAllTags() {
    return {
      tags: this.queries.getAllTags(),
    };
  }

  getActiveGoals() {
    return this.queries.getActiveGoals();
  }

  // Example: Print formatted output for CLI usage
  printUserDashboard(owner: string): void {
    const dashboard = this.getUserDashboard(owner);
    
    console.log('\n=== User Dashboard ===');
    console.log(`Owner: ${owner}\n`);
    
    console.log('Totals:');
    console.log(`  Savings Goals: ${dashboard.totals.total_goals} (Total: ${dashboard.totals.total_savings})`);
    console.log(`  Unpaid Bills: ${dashboard.totals.unpaid_bills} (Total: ${dashboard.totals.total_bills_amount})`);
    console.log(`  Active Policies: ${dashboard.totals.active_policies} (Coverage: ${dashboard.totals.total_coverage})\n`);
    
    if (dashboard.savings_goals.length > 0) {
      console.log('Savings Goals:');
      dashboard.savings_goals.forEach(goal => {
        const tags = JSON.parse(goal.tags);
        console.log(`  [${goal.id}] ${goal.name}: ${goal.current_amount}/${goal.target_amount} ${tags.length > 0 ? `[${tags.join(', ')}]` : ''}`);
      });
      console.log('');
    }
    
    if (dashboard.unpaid_bills.length > 0) {
      console.log('Unpaid Bills:');
      dashboard.unpaid_bills.forEach(bill => {
        const tags = JSON.parse(bill.tags);
        const dueDate = new Date(bill.due_date * 1000).toLocaleDateString();
        console.log(`  [${bill.id}] ${bill.name}: ${bill.amount} (Due: ${dueDate}) ${tags.length > 0 ? `[${tags.join(', ')}]` : ''}`);
      });
      console.log('');
    }
    
    if (dashboard.active_policies.length > 0) {
      console.log('Active Policies:');
      dashboard.active_policies.forEach(policy => {
        const tags = JSON.parse(policy.tags);
        console.log(`  [${policy.id}] ${policy.name} (${policy.coverage_type}): ${policy.coverage_amount} ${tags.length > 0 ? `[${tags.join(', ')}]` : ''}`);
      });
      console.log('');
    }
  }

  printOverdueBills(): void {
    const bills = this.getOverdueBills();
    
    console.log('\n=== Overdue Bills ===');
    if (bills.length === 0) {
      console.log('No overdue bills\n');
      return;
    }
    
    bills.forEach(bill => {
      const tags = JSON.parse(bill.tags);
      const dueDate = new Date(bill.due_date * 1000).toLocaleDateString();
      console.log(`[${bill.id}] ${bill.name}: ${bill.amount} (Due: ${dueDate}) - Owner: ${bill.owner} ${tags.length > 0 ? `[${tags.join(', ')}]` : ''}`);
    });
    console.log('');
  }

  printEntitiesByTag(tag: string): void {
    const entities = this.getEntitiesByTag(tag);
    
    console.log(`\n=== Entities Tagged: ${tag} ===\n`);
    
    if (entities.goals.length > 0) {
      console.log('Savings Goals:');
      entities.goals.forEach(goal => {
        console.log(`  [${goal.id}] ${goal.name}: ${goal.current_amount}/${goal.target_amount}`);
      });
      console.log('');
    }
    
    if (entities.bills.length > 0) {
      console.log('Bills:');
      entities.bills.forEach(bill => {
        console.log(`  [${bill.id}] ${bill.name}: ${bill.amount}`);
      });
      console.log('');
    }
    
    if (entities.policies.length > 0) {
      console.log('Insurance Policies:');
      entities.policies.forEach(policy => {
        console.log(`  [${policy.id}] ${policy.name}: ${policy.coverage_amount}`);
      });
      console.log('');
    }
    
    if (entities.goals.length === 0 && entities.bills.length === 0 && entities.policies.length === 0) {
      console.log('No entities found with this tag\n');
    }
  }

  printAllTags(): void {
    const result = this.getAllTags();
    
    console.log('\n=== All Tags ===');
    if (result.tags.length === 0) {
      console.log('No tags found\n');
      return;
    }
    
    result.tags.forEach(tag => {
      console.log(`  - ${tag}`);
    });
    console.log('');
  }
}
