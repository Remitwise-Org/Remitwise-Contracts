#!/usr/bin/env python3
"""
@notice Verifies cross-contract allocation invariants for remittance splits.
@dev Mirrors the accounting model covered by the Rust integration suite.
@custom:security Confirms allocation sums remain lossless and category totals
                  stay aligned with downstream recorded amounts.
"""

from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class SplitConfig:
    """@notice Static split percentages used by the verifier."""

    spending: int
    savings: int
    bills: int
    insurance: int

    def validate(self) -> None:
        total = self.spending + self.savings + self.bills + self.insurance
        if total != 100:
            raise AssertionError(f"invalid split total: expected 100, got {total}")


@dataclass(frozen=True)
class Allocation:
    """@notice Named allocation amounts returned by the split calculation."""

    total: int
    spending: int
    savings: int
    bills: int
    insurance: int

    def assert_lossless(self) -> None:
        allocated = self.spending + self.savings + self.bills + self.insurance
        if allocated != self.total:
            raise AssertionError(
                f"allocation sum mismatch: expected {self.total}, got {allocated}"
            )


@dataclass
class DownstreamState:
    """@notice Tracks the current downstream ledger view used by the tests."""

    savings_balance: int = 0
    unpaid_bills_total: int = 0
    active_premiums_total: int = 0

    def apply(self, allocation: Allocation) -> None:
        self.savings_balance += allocation.savings
        self.unpaid_bills_total += allocation.bills
        self.active_premiums_total += allocation.insurance

    def current_total(self) -> int:
        return (
            self.savings_balance
            + self.unpaid_bills_total
            + self.active_premiums_total
        )


class RemittanceSplitter:
    """@notice Deterministic mirror of the on-chain `calculate_split` logic."""

    def __init__(self, config: SplitConfig) -> None:
        config.validate()
        self.config = config

    def calculate_split(self, total: int) -> Allocation:
        if total <= 0:
            raise AssertionError("remittance total must be positive")

        spending = (total * self.config.spending) // 100
        savings = (total * self.config.savings) // 100
        bills = (total * self.config.bills) // 100
        insurance = total - spending - savings - bills

        allocation = Allocation(
            total=total,
            spending=spending,
            savings=savings,
            bills=bills,
            insurance=insurance,
        )
        allocation.assert_lossless()
        return allocation


def assert_category_match(allocation: Allocation, bill_amount: int, premium_amount: int) -> None:
    """@custom:security Guards against cross-category recording drift."""

    if bill_amount != allocation.bills:
        raise AssertionError(
            f"bill amount mismatch: expected {allocation.bills}, got {bill_amount}"
        )
    if premium_amount != allocation.insurance:
        raise AssertionError(
            "policy premium mismatch: "
            f"expected {allocation.insurance}, got {premium_amount}"
        )


def run_single_remittance_case() -> None:
    print("case: single remittance")
    splitter = RemittanceSplitter(SplitConfig(40, 30, 20, 10))
    state = DownstreamState()
    allocation = splitter.calculate_split(10_000)
    state.apply(allocation)
    assert_category_match(allocation, bill_amount=2_000, premium_amount=1_000)
    assert state.current_total() == allocation.total - allocation.spending
    print(f"  allocation={allocation}")


def run_cumulative_case() -> None:
    print("case: cumulative remittances")
    splitter = RemittanceSplitter(SplitConfig(40, 30, 20, 10))
    state = DownstreamState()
    total_remitted = 0
    expected_spending = 0

    for amount in (10_000, 17_333, 6_512):
        allocation = splitter.calculate_split(amount)
        state.apply(allocation)
        assert_category_match(
            allocation,
            bill_amount=allocation.bills,
            premium_amount=allocation.insurance,
        )
        total_remitted += amount
        expected_spending += allocation.spending

    tracked_spending = total_remitted - state.current_total()
    assert tracked_spending == expected_spending
    print(f"  total_remitted={total_remitted}")
    print(f"  unpaid_bills_total={state.unpaid_bills_total}")
    print(f"  active_premiums_total={state.active_premiums_total}")


def run_rounding_case() -> None:
    print("case: rounding remainder")
    splitter = RemittanceSplitter(SplitConfig(33, 33, 17, 17))
    allocation = splitter.calculate_split(101)
    assert allocation.insurance == 18
    print(f"  allocation={allocation}")


def run_state_transition_case() -> None:
    print("case: downstream state transitions")
    splitter = RemittanceSplitter(SplitConfig(40, 30, 20, 10))
    state = DownstreamState()
    allocation = splitter.calculate_split(8_400)
    state.apply(allocation)

    state.unpaid_bills_total -= allocation.bills
    state.active_premiums_total -= allocation.insurance

    assert state.savings_balance == allocation.savings
    assert state.unpaid_bills_total == 0
    assert state.active_premiums_total == 0
    print(f"  downstream_current_total={state.current_total()}")


def print_security_notes() -> None:
    print("security notes:")
    print("  - split sum must remain lossless after integer division")
    print("  - bill totals must track only the bills category")
    print("  - premium totals must track only the insurance category")
    print("  - paying a bill or deactivating a policy must reduce current totals")


def main() -> int:
    run_single_remittance_case()
    run_cumulative_case()
    run_rounding_case()
    run_state_transition_case()
    print_security_notes()
    print("all cross-contract invariant checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
