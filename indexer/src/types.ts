/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

// Database entity types
export interface SavingsGoal {
  id: number;
  owner: string;
  name: string;
  target_amount: string;
  current_amount: string;
  target_date: number;
  locked: boolean;
  unlock_date: number | null;
  tags: string;
  created_at: number;
  updated_at: number;
}

export interface Bill {
  id: number;
  owner: string;
  name: string;
  amount: string;
  due_date: number;
  recurring: boolean;
  frequency_days: number;
  paid: boolean;
  created_at: number;
  paid_at: number | null;
  schedule_id: number | null;
  tags: string;
  updated_at: number;
}

export interface InsurancePolicy {
  id: number;
  owner: string;
  name: string;
  coverage_type: string;
  monthly_premium: string;
  coverage_amount: string;
  active: boolean;
  next_payment_date: number;
  schedule_id: number | null;
  tags: string;
  created_at: number;
  updated_at: number;
}

export interface RemittanceSplit {
  id: number;
  owner: string;
  name: string;
  total_amount: string;
  recipients: string;
  executed: boolean;
  created_at: number;
  executed_at: number | null;
  updated_at: number;
}

export interface Event {
  id: number;
  ledger: number;
  tx_hash: string;
  contract_address: string;
  event_type: string;
  topic: string;
  data: string;
  timestamp: number;
}

// Event data types
export interface GoalCreatedEvent {
  goal_id: number;
  owner: string;
  name: string;
  target_amount: string;
  target_date: number;
}

export interface BillCreatedEvent {
  bill_id: number;
  owner: string;
  name: string;
  amount: string;
  due_date: number;
  recurring: boolean;
}

export interface PolicyCreatedEvent {
  policy_id: number;
  owner: string;
  name: string;
  coverage_type: string;
  monthly_premium: string;
}

export interface TagsAddedEvent {
  entity_id: number;
  owner: string;
  tags: string[];
}

export interface TagsRemovedEvent {
  entity_id: number;
  owner: string;
  tags: string[];
}
