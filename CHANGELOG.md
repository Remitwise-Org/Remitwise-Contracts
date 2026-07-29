# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html) and [Conventional Commits](https://www.conventionalcommits.org/).

*Note for downstream integrators: This changelog focuses on breaking changes, new entrypoints, and event structures that affect how external applications interact with the smart contracts.*

## [Unreleased]

### Added
- **feat(bill_payments):** implement add recurring bill schedule lifecycle.
  Integrators can now create automated recurring bill schedules that spawn new bills automatically.
  *Example:*
  ```rust
  let schedule_id = client.create_bill_schedule(
      &owner,
      &String::from_str(&env, "Electric Bill"),
      &150_0000000,
      &Symbol::new(&env, "USDC"),
      &1672531200, // next_due timestamp
      &2592000,    // interval (30 days in seconds)
  );
  ```
- **feat(killswitch):** add `get_unpause_schedule`, `list_paused_functions`, and `is_module_paused` entrypoints.
  Allows integrators to query the emergency pause state of contract modules.
  *Example Output:*
  ```rust
  let paused = killswitch_client.is_module_paused(&Symbol::new(&env, "bill_payments"));
  // Returns: true
  ```
- **docs(reporting):** add reporting admin rotation handoff window documentation.

### Changed
- **refactor(reporting):** unify period range validation in `remitwise-common`. Integrators shouldn't see functional differences, but error codes for invalid periods are now standardized to `Error::InvalidPeriod`.
- **security:** enforce dependency policy for GPL and yanked crates.

### Fixed
- **fix(killswitch):** upgrade runbook steps fixed for emergency handover.
