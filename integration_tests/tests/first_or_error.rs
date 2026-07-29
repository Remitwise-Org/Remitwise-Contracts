use reporting::first_or_error;
use soroban_sdk::{Env, Vec};

#[test]
fn first_or_error_returns_error_for_empty_input() {
    let env = Env::default();
    let values: Vec<u32> = Vec::new(&env);

    assert!(first_or_error(&values).is_err());
}

#[test]
fn first_or_error_returns_single_input_value() {
    let env = Env::default();
    let mut values = Vec::new(&env);
    values.push_back(42_u32);

    assert_eq!(first_or_error(&values).unwrap(), 42_u32);
}

#[test]
fn first_or_error_returns_first_value_from_many_inputs() {
    let env = Env::default();
    let mut values = Vec::new(&env);
    values.push_back(7_u32);
    values.push_back(14_u32);
    values.push_back(21_u32);

    assert_eq!(first_or_error(&values).unwrap(), 7_u32);
}
