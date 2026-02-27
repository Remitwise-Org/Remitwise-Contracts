/*
 * Copyright (c) 2026 Remitwise
 *
 * This source code is licensed under the MIT license found in the
 * LICENSE file in the root directory of this source tree.
 */

import { Server, SorobanRpc } from '@stellar/stellar-sdk';
import Database from 'better-sqlite3';
import { EventProcessor } from './eventProcessor';

export class Indexer {
  private server: Server;
  private processor: EventProcessor;
  private contracts: string[];
  private pollInterval: number;
  private isRunning: boolean = false;

  constructor(
    private db: Database.Database,
    rpcUrl: string,
    contracts: string[],
    pollIntervalMs: number = 5000
  ) {
    this.server = new Server(rpcUrl);
    this.processor = new EventProcessor(db);
    this.contracts = contracts;
    this.pollInterval = pollIntervalMs;
  }

  async start(): Promise<void> {
    console.log('Starting indexer...');
    console.log('Monitoring contracts:', this.contracts);
    
    this.isRunning = true;
    
    while (this.isRunning) {
      try {
        await this.poll();
        await this.sleep(this.pollInterval);
      } catch (error) {
        console.error('Error during polling:', error);
        await this.sleep(this.pollInterval);
      }
    }
  }

  stop(): void {
    console.log('Stopping indexer...');
    this.isRunning = false;
  }

  private async poll(): Promise<void> {
    const lastLedger = this.getLastProcessedLedger();
    const startLedger = lastLedger + 1;

    try {
      // Get latest ledger
      const latestLedger = await this.server.getLatestLedger();
      const currentLedger = latestLedger.sequence;

      if (startLedger > currentLedger) {
        // No new ledgers to process
        return;
      }

      console.log(`Processing ledgers ${startLedger} to ${currentLedger}`);

      // Process each contract
      for (const contractId of this.contracts) {
        await this.processContractEvents(contractId, startLedger, currentLedger);
      }

      // Update last processed ledger
      this.setLastProcessedLedger(currentLedger);
      
    } catch (error) {
      console.error('Error polling events:', error);
    }
  }

  private async processContractEvents(
    contractId: string,
    startLedger: number,
    endLedger: number
  ): Promise<void> {
    try {
      const response = await this.server.getEvents({
        startLedger,
        filters: [
          {
            type: 'contract',
            contractIds: [contractId],
          },
        ],
      });

      if (!response.events || response.events.length === 0) {
        return;
      }

      console.log(`Found ${response.events.length} events for contract ${contractId.substring(0, 8)}...`);

      for (const event of response.events) {
        this.processEvent(event, contractId);
      }
    } catch (error) {
      console.error(`Error fetching events for contract ${contractId}:`, error);
    }
  }

  private processEvent(event: any, contractId: string): void {
    try {
      const ledger = event.ledger;
      const txHash = event.txHash || 'unknown';
      const timestamp = this.ledgerToTimestamp(ledger);

      this.processor.processEvent(
        ledger,
        txHash,
        contractId,
        event,
        timestamp
      );
    } catch (error) {
      console.error('Error processing event:', error);
    }
  }

  private ledgerToTimestamp(ledger: number): number {
    // Stellar ledgers close approximately every 5 seconds
    // Genesis ledger was at 2015-09-30T16:00:00Z (1443628800)
    const GENESIS_TIMESTAMP = 1443628800;
    const LEDGER_CLOSE_TIME = 5;
    
    return GENESIS_TIMESTAMP + (ledger * LEDGER_CLOSE_TIME);
  }

  private getLastProcessedLedger(): number {
    const stmt = this.db.prepare('SELECT value FROM indexer_state WHERE key = ?');
    const result = stmt.get('last_ledger') as any;
    
    if (result) {
      return parseInt(result.value);
    }
    
    // Return start ledger from env or 0
    return parseInt(process.env.START_LEDGER || '0');
  }

  private setLastProcessedLedger(ledger: number): void {
    const stmt = this.db.prepare(`
      INSERT OR REPLACE INTO indexer_state (key, value)
      VALUES (?, ?)
    `);
    stmt.run('last_ledger', ledger.toString());
  }

  private sleep(ms: number): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, ms));
  }
}
