#!/usr/bin/env python3
"""
Cross-Contract Invariant Verification Script

This script verifies that allocations across savings, bills, and insurance
are consistent with the remittance split over time.

Issue: #154 - Cross-Contract Invariant Checks (Split vs Allocations)
"""

import sys
from typing import List, Dict, Tuple
from dataclasses import dataclass


@dataclass
class SplitConfig:
    """Remittance split configuration"""
    spending_percent: int
    savings_percent: int
    bills_percent: int
    insurance_percent: int

    def validate(self) -> bool:
        """Validate that percentages sum to 100"""
        total = (self.spending_percent + self.savings_percent + 
                self.bills_percent + self.insurance_percent)
        return total == 100


@dataclass
class AllocationResult:
    """Result of a remittance allocation"""
    total: int
    spending: int
    savings: int
    bills: int
    insurance: int

    def sum_allocations(self) -> int:
        """Sum of all allocations"""
        return self.spending + self.savings + self.bills + self.insurance

    def is_consistent(self) -> bool:
        """Check if allocations sum to total"""
        return self.sum_allocations() == self.total


class RemittanceSplitter:
    """Simulates the remittance split contract"""
    
    def __init__(self, config: SplitConfig):
        if not config.validate():
            raise ValueError("Split percentages must sum to 100")
        self.config = config

    def calculate_split(self, total_amount: int) -> AllocationResult:
        """Calculate split amounts from total remittance"""
        if total_amount <= 0:
            raise ValueError("Total amount must be positive")

        # Calculate each allocation
        spending = (total_amount * self.config.spending_percent) // 100
        savings = (total_amount * self.config.savings_percent) // 100
        bills = (total_amount * self.config.bills_percent) // 100
        
        # Allocate remainder to insurance to avoid rounding loss
        insurance = total_amount - spending - savings - bills

        return AllocationResult(
            total=total_amount,
            spending=spending,
            savings=savings,
            bills=bills,
            insurance=insurance
        )


class ContractTracker:
    """Tracks allocations to each contract"""
    
    def __init__(self):
        self.savings_total = 0
        self.bills_total = 0
        self.insurance_total = 0
        self.spending_total = 0

    def allocate(self, result: AllocationResult):
        """Record an allocation"""
        self.spending_total += result.spending
        self.savings_total += result.savings
        self.bills_total += result.bills
        self.insurance_total += result.insurance

    def get_total_allocated(self) -> int:
        """Get total allocated across all contracts"""
        return (self.spending_total + self.savings_total + 
                self.bills_total + self.insurance_total)

    def get_tracked_total(self) -> int:
        """Get total tracked (excluding spending)"""
        return self.savings_total + self.bills_total + self.insurance_total


def test_single_remittance():
    """Test single remittance allocation consistency"""
    print("Test 1: Single Remittance Allocation Consistency")
    print("-" * 60)
    
    config = SplitConfig(40, 30, 20, 10)
    splitter = RemittanceSplitter(config)
    tracker = ContractTracker()
    
    amount = 10_000_0000000  # 10,000 XLM (7 decimals)
    result = splitter.calculate_split(amount)
    tracker.allocate(result)
    
    print(f"Total Remittance: {amount:,}")
    print(f"Spending:  {result.spending:,} ({config.spending_percent}%)")
    print(f"Savings:   {result.savings:,} ({config.savings_percent}%)")
    print(f"Bills:     {result.bills:,} ({config.bills_percent}%)")
    print(f"Insurance: {result.insurance:,} ({config.insurance_percent}%)")
    print(f"Sum:       {result.sum_allocations():,}")
    
    assert result.is_consistent(), "Allocations must sum to total"
    assert tracker.get_total_allocated() == amount, "Tracked total must match remittance"
    
    print("✅ PASS: Single remittance allocation is consistent\n")
    return True


def test_multiple_remittances():
    """Test multiple remittances allocation consistency"""
    print("Test 2: Multiple Remittances Allocation Consistency")
    print("-" * 60)
    
    config = SplitConfig(40, 30, 20, 10)
    splitter = RemittanceSplitter(config)
    tracker = ContractTracker()
    
    remittances = [
        5_000_0000000,   # 5,000 XLM
        10_000_0000000,  # 10,000 XLM
        7_500_0000000,   # 7,500 XLM
        15_000_0000000,  # 15,000 XLM
        3_000_0000000,   # 3,000 XLM
    ]
    
    total_remitted = 0
    
    for i, amount in enumerate(remittances, 1):
        result = splitter.calculate_split(amount)
        tracker.allocate(result)
        total_remitted += amount
        
        assert result.is_consistent(), f"Remittance {i} allocations must sum to total"
        print(f"Remittance {i}: {amount:,} -> {result.sum_allocations():,} ✓")
    
    print(f"\nTotal Remitted:  {total_remitted:,}")
    print(f"Total Allocated: {tracker.get_total_allocated():,}")
    print(f"Spending Total:  {tracker.spending_total:,}")
    print(f"Savings Total:   {tracker.savings_total:,}")
    print(f"Bills Total:     {tracker.bills_total:,}")
    print(f"Insurance Total: {tracker.insurance_total:,}")
    
    assert tracker.get_total_allocated() == total_remitted, \
        "Cumulative allocations must equal total remitted"
    
    print("✅ PASS: Multiple remittances maintain consistency\n")
    return True


def test_rounding_edge_cases():
    """Test amounts that might cause rounding issues"""
    print("Test 3: Rounding Edge Cases")
    print("-" * 60)
    
    config = SplitConfig(40, 30, 20, 10)
    splitter = RemittanceSplitter(config)
    
    test_amounts = [
        (1, "Minimum amount"),
        (7, "Prime number"),
        (99, "Just under 100"),
        (1_000_0000000, "1,000 XLM"),
        (3_333_0000000, "Doesn't divide evenly"),
        (9_999_9999999, "Large odd number"),
    ]
    
    all_passed = True
    
    for amount, description in test_amounts:
        result = splitter.calculate_split(amount)
        is_consistent = result.is_consistent()
        status = "✓" if is_consistent else "✗"
        
        print(f"{status} {description:25} Amount: {amount:15,} Sum: {result.sum_allocations():15,}")
        
        if not is_consistent:
            print(f"  ERROR: Allocations don't sum to total!")
            print(f"  Expected: {amount}, Got: {result.sum_allocations()}")
            all_passed = False
    
    assert all_passed, "All edge cases must maintain consistency"
    print("✅ PASS: All rounding edge cases handled correctly\n")
    return True


def test_high_volume():
    """Test high-volume processing"""
    print("Test 4: High Volume Allocation Consistency")
    print("-" * 60)
    
    config = SplitConfig(40, 30, 20, 10)
    splitter = RemittanceSplitter(config)
    tracker = ContractTracker()
    
    num_remittances = 100
    base_amount = 5_000_0000000  # 5,000 XLM
    
    total_remitted = 0
    
    for i in range(num_remittances):
        amount = base_amount + (i * 100_0000000)  # Vary amounts
        result = splitter.calculate_split(amount)
        tracker.allocate(result)
        total_remitted += amount
        
        assert result.is_consistent(), f"Remittance {i+1} must be consistent"
    
    print(f"Processed: {num_remittances} remittances")
    print(f"Total Remitted:  {total_remitted:,}")
    print(f"Total Allocated: {tracker.get_total_allocated():,}")
    
    # Calculate expected tracked (savings + bills + insurance = 60%)
    tracked_pct = config.savings_percent + config.bills_percent + config.insurance_percent
    expected_tracked = (total_remitted * tracked_pct) // 100
    actual_tracked = tracker.get_tracked_total()
    
    # Allow small tolerance for cumulative rounding (< 0.01%)
    tolerance = total_remitted // 10000
    difference = abs(actual_tracked - expected_tracked)
    
    print(f"Expected Tracked: {expected_tracked:,}")
    print(f"Actual Tracked:   {actual_tracked:,}")
    print(f"Difference:       {difference:,}")
    print(f"Tolerance:        {tolerance:,}")
    
    assert tracker.get_total_allocated() == total_remitted, \
        "Total allocated must equal total remitted"
    assert difference <= tolerance, \
        f"Tracked allocations should match expected within tolerance"
    
    print("✅ PASS: High volume processing maintains consistency\n")
    return True


def test_percentage_maintenance():
    """Test that percentages are maintained across multiple remittances"""
    print("Test 5: Percentage Maintenance")
    print("-" * 60)
    
    config = SplitConfig(40, 30, 20, 10)
    splitter = RemittanceSplitter(config)
    tracker = ContractTracker()
    
    remittances = [
        10_000_0000000,
        20_000_0000000,
        15_000_0000000,
    ]
    
    total_remitted = 0
    
    for amount in remittances:
        result = splitter.calculate_split(amount)
        tracker.allocate(result)
        total_remitted += amount
    
    # Calculate actual percentages
    actual_spending_pct = (tracker.spending_total * 100) // total_remitted
    actual_savings_pct = (tracker.savings_total * 100) // total_remitted
    actual_bills_pct = (tracker.bills_total * 100) // total_remitted
    actual_insurance_pct = (tracker.insurance_total * 100) // total_remitted
    
    print(f"Expected Percentages: {config.spending_percent}% / {config.savings_percent}% / {config.bills_percent}% / {config.insurance_percent}%")
    print(f"Actual Percentages:   {actual_spending_pct}% / {actual_savings_pct}% / {actual_bills_pct}% / {actual_insurance_pct}%")
    
    # Allow 1% tolerance for rounding
    tolerance = 1
    
    assert abs(actual_spending_pct - config.spending_percent) <= tolerance, \
        "Spending percentage should be maintained"
    assert abs(actual_savings_pct - config.savings_percent) <= tolerance, \
        "Savings percentage should be maintained"
    assert abs(actual_bills_pct - config.bills_percent) <= tolerance, \
        "Bills percentage should be maintained"
    assert abs(actual_insurance_pct - config.insurance_percent) <= tolerance, \
        "Insurance percentage should be maintained"
    
    print("✅ PASS: Percentages maintained within tolerance\n")
    return True


def main():
    """Run all invariant tests"""
    print("=" * 60)
    print("Cross-Contract Invariant Verification")
    print("Issue #154: Split vs Allocations Consistency")
    print("=" * 60)
    print()
    
    tests = [
        test_single_remittance,
        test_multiple_remittances,
        test_rounding_edge_cases,
        test_high_volume,
        test_percentage_maintenance,
    ]
    
    passed = 0
    failed = 0
    
    for test in tests:
        try:
            if test():
                passed += 1
        except AssertionError as e:
            print(f"❌ FAIL: {e}\n")
            failed += 1
        except Exception as e:
            print(f"❌ ERROR: {e}\n")
            failed += 1
    
    print("=" * 60)
    print(f"Results: {passed} passed, {failed} failed")
    print("=" * 60)
    
    if failed == 0:
        print("\n✅ All invariant checks PASSED")
        print("No discrepancies found between remittance splits and allocations")
        return 0
    else:
        print(f"\n❌ {failed} test(s) FAILED")
        print("Discrepancies detected - see above for details")
        return 1


if __name__ == "__main__":
    sys.exit(main())
