/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

/**
 * Example queries demonstrating the indexer API
 * Run with: ts-node examples/query-examples.ts
 */

import dotenv from 'dotenv';
import { initializeDatabase } from '../src/db/schema';
import { ApiService } from '../src/api';

dotenv.config();

async function main() {
  // Initialize database connection
  const dbPath = process.env.DB_PATH || './data/remitwise.db';
  const db = initializeDatabase(dbPath);
  const api = new ApiService(db);

  console.log('=== Remitwise Indexer Query Examples ===\n');

  // Example 1: Get user dashboard
  console.log('Example 1: User Dashboard');
  console.log('-------------------------');
  const exampleOwner = 'GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX';
  const dashboard = api.getUserDashboard(exampleOwner);
  console.log(JSON.stringify(dashboard, null, 2));
  console.log('\n');

  // Example 2: Get all overdue bills
  console.log('Example 2: Overdue Bills');
  console.log('------------------------');
  const overdueBills = api.getOverdueBills();
  console.log(`Found ${overdueBills.length} overdue bills`);
  overdueBills.slice(0, 5).forEach(bill => {
    console.log(`  - ${bill.name}: ${bill.amount} (Due: ${new Date(bill.due_date * 1000).toLocaleDateString()})`);
  });
  console.log('\n');

  // Example 3: Get entities by tag
  console.log('Example 3: Entities by Tag');
  console.log('--------------------------');
  const tag = 'emergency';
  const entitiesByTag = api.getEntitiesByTag(tag);
  console.log(`Entities tagged with "${tag}":`);
  console.log(`  Goals: ${entitiesByTag.goals.length}`);
  console.log(`  Bills: ${entitiesByTag.bills.length}`);
  console.log(`  Policies: ${entitiesByTag.policies.length}`);
  console.log('\n');

  // Example 4: Get all unique tags
  console.log('Example 4: All Tags');
  console.log('-------------------');
  const allTags = api.getAllTags();
  console.log(`Total unique tags: ${allTags.tags.length}`);
  console.log(`Tags: ${allTags.tags.join(', ')}`);
  console.log('\n');

  // Example 5: Get active goals
  console.log('Example 5: Active Goals');
  console.log('-----------------------');
  const activeGoals = api.getActiveGoals();
  console.log(`Found ${activeGoals.length} active goals`);
  activeGoals.slice(0, 5).forEach(goal => {
    const progress = (parseFloat(goal.current_amount) / parseFloat(goal.target_amount) * 100).toFixed(1);
    console.log(`  - ${goal.name}: ${progress}% complete`);
  });
  console.log('\n');

  // Example 6: Custom query - Goals near completion
  console.log('Example 6: Goals Near Completion (>80%)');
  console.log('----------------------------------------');
  const allGoals = activeGoals.filter(goal => {
    const progress = parseFloat(goal.current_amount) / parseFloat(goal.target_amount);
    return progress >= 0.8;
  });
  console.log(`Found ${allGoals.length} goals near completion`);
  allGoals.forEach(goal => {
    const progress = (parseFloat(goal.current_amount) / parseFloat(goal.target_amount) * 100).toFixed(1);
    console.log(`  - ${goal.name}: ${progress}% (${goal.current_amount}/${goal.target_amount})`);
  });
  console.log('\n');

  // Close database
  db.close();
  console.log('Examples completed!');
}

main().catch(error => {
  console.error('Error running examples:', error);
  process.exit(1);
});
