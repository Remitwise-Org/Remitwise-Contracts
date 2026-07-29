import Database from 'better-sqlite3';
import { getLastProcessedLedger, updateLastProcessedLedger } from '../src/db/queries';

describe('Indexer Cursor State', () => {
  let db: any;
  
  beforeAll(() => {
    db = new (Database as any)(':memory:');
    db.exec('CREATE TABLE indexer_state (id INTEGER PRIMARY KEY CHECK (id = 1), last_processed_ledger INTEGER NOT NULL);');
  });

  it('should persist and resume cursor correctly to prevent gaps', () => {
    updateLastProcessedLedger(db, 100);
    expect(getLastProcessedLedger(db)).toBe(100);
    
    updateLastProcessedLedger(db, 105);
    expect(getLastProcessedLedger(db)).toBe(105);
  });
});
