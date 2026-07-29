# SC-006 Bill Payments: Owner-Indexed Archived Pagination

## TODO List

- [x] 1. Analyze codebase and understand existing implementation
- [x] 2. Create plan and get user approval
- [x] 3. Edit `bill_payments/Cargo.toml` - Add test target for `archived_pagination_tests.rs`
- [x] 4. Edit `bill_payments/src/lib.rs` - Refactor `get_archived_bills_page` to use `get_owner_archived_bills` and remove duplicate `get_owner_index`
- [x] 5. Edit `bill_payments/src/lib.rs` - Fix `adjust_unpaid_total` function signature (was damaged during previous edit)
- [x] 6. Review all changes for correctness

