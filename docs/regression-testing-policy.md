# Regression Testing Policy

- Every bug fix must include a failing regression test that reproduces the issue and passes with the fix.
- Place new regression tests alongside the affected crate’s tests or under integration_tests in a dedicated regression_tests module.
- Tests must be minimal, deterministic, and focus on the fixed behavior.
- Do not modify production code solely to accommodate tests.
- Link the test to its issue or PR number in the commit message.
- CI must block merges for bug-fix PRs without a corresponding regression test.

Scope
- Applies to all crates in this workspace.
- Includes unit, integration, and property-based tests where appropriate.

Process
- Reproduce: Write a test that fails against the buggy behavior.
- Fix: Implement the code change.
- Verify: Ensure the new test and the full suite pass locally and in CI.
- Document: Reference the issue/PR in the test commit message.
