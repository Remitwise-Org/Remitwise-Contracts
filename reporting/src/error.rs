use soroban_sdk::{contracterror, ContractError};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ReportingError {
    /// Addresses are not configured in storage
    AddressesNotConfigured = 1,
    /// Invalid address provided
    InvalidAddress = 2,
    /// Unauthorized caller
    Unauthorized = 3,
    /// Invalid reporting period
    InvalidPeriod = 4,
    /// No data available for the requested period
    DataAvailabilityMissing = 5,
    /// Other configuration error
    ConfigurationError = 6,
}