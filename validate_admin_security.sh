#!/bin/bash

# Admin Role Security Validation Script
# This script validates that admin role transfer security is properly implemented

echo "=== RemitWise Admin Role Security Validation ==="
echo

# Check if all contracts have the required admin functions
echo "1. Checking admin function implementations..."

contracts=("bill_payments" "insurance" "savings_goals" "remittance_split" "family_wallet")

for contract in "${contracts[@]}"; do
    echo "  Checking $contract..."
    
    # Check for set_upgrade_admin function
    if grep -q "pub fn set_upgrade_admin" "$contract/src/lib.rs"; then
        echo "    ✅ set_upgrade_admin function found"
    else
        echo "    ❌ set_upgrade_admin function missing"
    fi
    
    # Check for get_upgrade_admin_public function
    if grep -q "pub fn get_upgrade_admin_public" "$contract/src/lib.rs"; then
        echo "    ✅ get_upgrade_admin_public function found"
    else
        echo "    ❌ get_upgrade_admin_public function missing"
    fi
    
    # Check for authorization logic
    if grep -q "caller != new_admin" "$contract/src/lib.rs"; then
        echo "    ✅ Bootstrap authorization check found"
    else
        echo "    ⚠️  Bootstrap authorization check not found (may use different pattern)"
    fi
    
    # Check for event emission
    if grep -q "adm_xfr" "$contract/src/lib.rs"; then
        echo "    ✅ Admin transfer event emission found"
    else
        echo "    ❌ Admin transfer event emission missing"
    fi
    
    echo
done

echo "2. Checking integration test implementation..."

if [ -f "integration_tests/tests/multi_contract_integration.rs" ]; then
    echo "  ✅ Multi-contract integration tests found"
    
    # Check for key test functions
    test_functions=(
        "test_bootstrap_admin_setup_all_contracts"
        "test_unauthorized_bootstrap_attempts" 
        "test_authorized_admin_transfer"
        "test_unauthorized_admin_transfer"
        "test_admin_operations_while_paused"
        "test_version_upgrade_authorization"
        "test_cross_contract_admin_isolation"
    )
    
    for test_func in "${test_functions[@]}"; do
        if grep -q "$test_func" "integration_tests/tests/multi_contract_integration.rs"; then
            echo "    ✅ $test_func found"
        else
            echo "    ❌ $test_func missing"
        fi
    done
else
    echo "  ❌ Multi-contract integration tests missing"
fi

echo

echo "3. Checking documentation..."

if [ -f "ADMIN_ROLE_SECURITY.md" ]; then
    echo "  ✅ Admin role security documentation found"
else
    echo "  ❌ Admin role security documentation missing"
fi

if grep -q "Admin Role Transfer Security" "UPGRADE_GUIDE.md"; then
    echo "  ✅ Upgrade guide updated with admin security section"
else
    echo "  ❌ Upgrade guide missing admin security section"
fi

echo

echo "4. Security pattern validation..."

echo "  Checking for consistent error handling patterns..."
for contract in "${contracts[@]}"; do
    if grep -q "Unauthorized" "$contract/src/lib.rs"; then
        echo "    ✅ $contract: Unauthorized error handling found"
    else
        echo "    ⚠️  $contract: Unauthorized error handling not found"
    fi
done

echo

echo "5. Recommendations..."
echo "  - Run 'cargo test -p integration_tests' to execute regression tests"
echo "  - Review ADMIN_ROLE_SECURITY.md for detailed security analysis"
echo "  - Ensure all contracts compile successfully before deployment"
echo "  - Validate admin transfer events in testnet deployment"

echo
echo "=== Validation Complete ==="