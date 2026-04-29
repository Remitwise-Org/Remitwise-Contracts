import re
import os

file_path = '/home/chukwuemekadr/Documents/Drips/Wave4/Remitwise-Contracts/reporting/src/lib.rs'

with open(file_path, 'r') as f:
    content = f.read()

# Fix _internal functions return types and Ok() wrapping
functions = [
    'get_remittance_summary_internal',
    'get_savings_report_internal',
    'get_bill_compliance_report_internal',
    'get_insurance_report_internal',
    'calculate_health_score_internal'
]

return_types = {
    'get_remittance_summary_internal': 'Result<RemittanceSummary, ReportingError>',
    'get_savings_report_internal': 'Result<SavingsReport, ReportingError>',
    'get_bill_compliance_report_internal': 'Result<BillComplianceReport, ReportingError>',
    'get_insurance_report_internal': 'Result<InsuranceReport, ReportingError>',
    'calculate_health_score_internal': 'Result<HealthScore, ReportingError>'
}

for func in functions:
    # Update return type
    pattern = rf'fn {func}\(\s*([^)]*)\s*\)\s*->\s*[^ {{]+'
    content = re.sub(pattern, f'fn {func}(\\1) -> {return_types[func]}', content)

# Fix Ok(...) wrapping with missing closing parenthesis
content = re.sub(r'Ok\(([^{}]+\{[^{}]+\})\s*\}', r'Ok(\1)\n    }', content)

# Fix public methods that were wrapping already Result-returning internal methods
public_methods = [
    'get_savings_report',
    'get_bill_compliance_report',
    'get_insurance_report',
    'calculate_health_score'
]

for method in public_methods:
    pattern = rf'Ok\(Self::{method}_internal\(([^)]*)\)\)'
    content = re.sub(pattern, f'Self::{method_name}_internal(\\1)' if 'method_name' in locals() else f'Self::{method}_internal(\\1)', content)

# Specific fix for calculate_health_score return type if needed
# It returns HealthScore, let's check.
# pub fn calculate_health_score(env: Env, user: Address, total_remittance: i128) -> HealthScore
# It should probably return Result<HealthScore, ReportingError> too if internal does.

with open(file_path, 'w') as f:
    f.write(content)
