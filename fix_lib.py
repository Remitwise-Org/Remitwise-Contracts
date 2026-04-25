import os

path = r'bill_payments\src\lib.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# Fix ArchivedBill struct and duplicate attributes
old_archived_bill = """#[derive(Clone)]
#[contracttype]
#[derive(Clone)]
#[contracttype]
pub struct ArchivedBill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub amount: i128,
    pub paid_at: u64,
    pub archived_at: u64,
    pub tags: Vec<String>,
    /// Intended currency/asset carried over from the originating `Bill`.
    pub currency: String,
}"""
new_archived_bill = """#[derive(Clone)]
#[contracttype]
pub struct ArchivedBill {
    pub id: u32,
    pub owner: Address,
    pub name: String,
    pub external_ref: Option<String>,
    pub amount: i128,
    pub paid_at: u64,
    pub archived_at: u64,
    pub tags: Vec<String>,
    /// Intended currency/asset carried over from the originating `Bill`.
    pub currency: String,
}"""

# Handle potential \r\n vs \n
def fix_replace(text, old, new):
    # Normalize everything to \n for the search, then try to match the original line endings
    if old in text:
        return text.replace(old, new)
    # try with \r\n
    old_rn = old.replace('\n', '\r\n')
    new_rn = new.replace('\n', '\r\n')
    if old_rn in text:
        return text.replace(old_rn, new_rn)
    # try mixed or just normalize then replace
    return text

content = fix_replace(content, old_archived_bill, new_archived_bill)

# Fix restore_bill instantiation
old_restore = """        let restored_bill = Bill {
            id: archived_bill.id,
            owner: archived_bill.owner.clone(),
            name: archived_bill.name.clone(),
            amount: archived_bill.amount,
            due_date: env.ledger().timestamp() + 2592000,
            recurring: false,
            frequency_days: 0,
            paid: true,
            created_at: archived_bill.paid_at,
            paid_at: Some(archived_bill.paid_at),
            schedule_id: None,
            tags: archived_bill.tags.clone(),
            currency: archived_bill.currency.clone(),
        };"""
new_restore = """        let restored_bill = Bill {
            id: archived_bill.id,
            owner: archived_bill.owner.clone(),
            name: archived_bill.name.clone(),
            external_ref: archived_bill.external_ref.clone(),
            amount: archived_bill.amount,
            due_date: env.ledger().timestamp() + 2592000,
            recurring: false,
            frequency_days: 0,
            paid: true,
            created_at: archived_bill.paid_at,
            paid_at: Some(archived_bill.paid_at),
            schedule_id: None,
            tags: archived_bill.tags.clone(),
            currency: archived_bill.currency.clone(),
        };"""
content = fix_replace(content, old_restore, new_restore)

# Fix batch_pay_bills instantiation
old_batch_pay = """                let next_bill = Bill {
                    id: next_id,
                    owner: bill.owner.clone(),
                    name: bill.name.clone(),
                    amount: bill.amount,
                    due_date: next_due_date,
                    recurring: true,
                    frequency_days: bill.frequency_days,
                    paid: false,
                    created_at: current_time,
                    paid_at: None,
                    schedule_id: bill.schedule_id,
                    tags: bill.tags.clone(),
                    currency: bill.currency.clone(),
                };"""
new_batch_pay = """                let next_bill = Bill {
                    id: next_id,
                    owner: bill.owner.clone(),
                    name: bill.name.clone(),
                    external_ref: bill.external_ref.clone(),
                    amount: bill.amount,
                    due_date: next_due_date,
                    recurring: true,
                    frequency_days: bill.frequency_days,
                    paid: false,
                    created_at: current_time,
                    paid_at: None,
                    schedule_id: bill.schedule_id,
                    tags: bill.tags.clone(),
                    currency: bill.currency.clone(),
                };"""
content = fix_replace(content, old_batch_pay, new_batch_pay)

# Fix redundant/broken get_all_bills
broken_get_all = """    pub fn get_all_bills(env: Env) -> Vec<Bill> {"""
clean_get_all = """"""
content = fix_replace(content, broken_get_all, clean_get_all)

with open(path, 'w', encoding='utf-8') as f:
    f.write(content)

print("Fixes applied successfully.")
