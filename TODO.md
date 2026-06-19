- [x] Implement deterministic tie-break for Top-N bills/savings reports (ID ascending on equal amounts/targets) in reporting/src/lib.rs
- [ ] Add Top-N tests for:
  - [ ] deterministic ordering across repeated calls
  - [ ] tie-break rule when amounts/targets are equal
  - [ ] cap enforcement at MAX_ITEMS_PER_REPORT and MAX_ITEMS_PER_REPORT+1
  - [ ] fewer than N and zero items
  - [ ] partial-data degradation (DataAvailability::Partial) under dependency pagination cap
- [x] Add/Update documentation under docs/ describing Top-N ordering contract + tie-break rule
- [ ] Run `cargo test -p reporting get_top -- --nocapture`
- [ ] Run `cargo test -p reporting`
- [ ] Run clippy for reporting (per repo standard)


