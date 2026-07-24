//! Centralized token registry for the Remitwise ecosystem.
//!
//! Every token the platform supports is declared here as a [`SupportedToken`]
//! variant. Adding a new token means:
//!
//! 1. Add a variant to [`SupportedToken`].
//! 2. Implement the metadata methods on [`SupportedToken`].
//! 3. The compiler will force every `match` in the workspace to handle the new
//!    variant — no consumer can silently ignore it.
//!
//! Contracts should import [`SupportedToken`] and its constants instead of
//! hardcoding currency strings, stroop multipliers, or decimal counts.

use soroban_sdk::contracttype;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Minor-unit exponent for the Stellar native asset (XLM).
pub const XLM_DECIMALS: u32 = 7;

/// Minor-unit exponent for USDC on Stellar.
pub const USDC_DECIMALS: u32 = 6;

/// Minor-unit exponent for EURC on Stellar.
pub const EURC_DECIMALS: u32 = 7;

/// Minor units per major unit for XLM (10^7 stroops).
pub const STROOPS_PER_XLM: i128 = 10_000_000;

/// Minor units per major unit for USDC (10^6 base units).
pub const BASE_UNITS_PER_USDC: i128 = 1_000_000;

/// Minor units per major unit for EURC (10^7 base units).
pub const BASE_UNITS_PER_EURC: i128 = 10_000_000;

/// Default currency code used when no currency is specified.
pub const DEFAULT_CURRENCY: &str = "XLM";

/// Maximum byte length of a user-supplied currency code string.
pub const MAX_CURRENCY_LEN: u32 = 10;

// ---------------------------------------------------------------------------
// SupportedToken enum
// ---------------------------------------------------------------------------

/// Every token the Remitwise platform recognises.
///
/// *Adding a token*: append a new `#[repr(u32)]` variant **at the end** to
/// preserve discriminant stability across upgrades. The compiler will then
/// require every exhaustive `match` in the workspace to handle the new case.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SupportedToken {
    XLM = 1,
    USDC = 2,
    EURC = 3,
}

impl SupportedToken {
    /// Number of decimal (minor-unit) places for this token.
    pub fn decimals(&self) -> u32 {
        match self {
            SupportedToken::XLM => XLM_DECIMALS,
            SupportedToken::USDC => USDC_DECIMALS,
            SupportedToken::EURC => EURC_DECIMALS,
        }
    }

    /// Minor units per one major unit (e.g. stroops per XLM).
    pub fn base_units_per_unit(&self) -> i128 {
        match self {
            SupportedToken::XLM => STROOPS_PER_XLM,
            SupportedToken::USDC => BASE_UNITS_PER_USDC,
            SupportedToken::EURC => BASE_UNITS_PER_EURC,
        }
    }

    /// Three-letter uppercase currency code.
    pub fn currency_code(&self) -> &'static str {
        match self {
            SupportedToken::XLM => "XLM",
            SupportedToken::USDC => "USDC",
            SupportedToken::EURC => "EURC",
        }
    }

    /// Try to parse a currency code string into a [`SupportedToken`].
    ///
    /// Matching is case-sensitive and expects the all-uppercase form
    /// (e.g. `"XLM"`, `"USDC"`, `"EURC"`).
    pub fn from_currency_code(code: &str) -> Option<Self> {
        match code {
            "XLM" => Some(SupportedToken::XLM),
            "USDC" => Some(SupportedToken::USDC),
            "EURC" => Some(SupportedToken::EURC),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimals_match_spec() {
        assert_eq!(SupportedToken::XLM.decimals(), 7);
        assert_eq!(SupportedToken::USDC.decimals(), 6);
        assert_eq!(SupportedToken::EURC.decimals(), 7);
    }

    #[test]
    fn base_units_match_spec() {
        assert_eq!(SupportedToken::XLM.base_units_per_unit(), 10_000_000);
        assert_eq!(SupportedToken::USDC.base_units_per_unit(), 1_000_000);
        assert_eq!(SupportedToken::EURC.base_units_per_unit(), 10_000_000);
    }

    #[test]
    fn currency_code_round_trip() {
        for token in [
            SupportedToken::XLM,
            SupportedToken::USDC,
            SupportedToken::EURC,
        ] {
            let code = token.currency_code();
            assert_eq!(SupportedToken::from_currency_code(code), Some(token));
        }
    }

    #[test]
    fn from_currency_code_rejects_unknown() {
        assert_eq!(SupportedToken::from_currency_code("NGN"), None);
        assert_eq!(SupportedToken::from_currency_code("xlm"), None);
        assert_eq!(SupportedToken::from_currency_code(""), None);
    }

    #[test]
    fn default_currency_is_xlm() {
        assert_eq!(DEFAULT_CURRENCY, "XLM");
    }

    #[test]
    fn max_currency_len_is_ten() {
        assert_eq!(MAX_CURRENCY_LEN, 10);
    }

    #[test]
    fn exhaustive_match_compiles() {
        fn _cover(token: SupportedToken) -> u32 {
            match token {
                SupportedToken::XLM => 1,
                SupportedToken::USDC => 2,
                SupportedToken::EURC => 3,
            }
        }
    }

    #[test]
    fn discriminant_stability() {
        assert_eq!(SupportedToken::XLM as u32, 1);
        assert_eq!(SupportedToken::USDC as u32, 2);
        assert_eq!(SupportedToken::EURC as u32, 3);
    }
}
