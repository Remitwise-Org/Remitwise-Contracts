use soroban_sdk::{Address, Env};
use crate::error::ReportingError;

/// Safely get configured addresses, returning a structured error if not set
pub fn get_configured_addresses(env: &Env) -> Result<Addresses, ReportingError> {
    // Example: get addresses from storage
    let addresses = storage::get_addresses(env)
        .ok_or(ReportingError::AddressesNotConfigured)?;
    
    // Validate addresses if needed
    if addresses.reporting_contract == Address::default(env) {
        return Err(ReportingError::AddressesNotConfigured);
    }
    
    Ok(addresses)
}

/// Safely get a single address
pub fn get_configured_address(env: &Env, key: &str) -> Result<Address, ReportingError> {
    let addr = storage::get_address(env, key)
        .ok_or(ReportingError::AddressesNotConfigured)?;
    
    if addr == Address::default(env) {
        return Err(ReportingError::AddressesNotConfigured);
    }
    
    Ok(addr)
}