#![cfg(test)]

use emergency_killswitch::{EmergencyKillswitch, EmergencyKillswitchClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, Symbol, TryFromVal,
};

/// Shared setup: register contract, mint admin, initialize, return (env, client, admin).
fn setup() -> (Env, EmergencyKillswitchClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, EmergencyKillswitch);
    let client = EmergencyKillswitchClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    (env, client, admin)
}

// ---------------------------------------------------------------------------
// 1. Mutating functions fail while paused — do_transfer
// ---------------------------------------------------------------------------
#[test]
fn test_pause_blocks_transfer() {
    let (_, client, admin) = setup();

    client.pause(&admin);
    assert!(client.is_paused(), "contract should be paused");

    let user = Address::generate(&client.env);
    let result = client.try_do_transfer(&user, &500);
    assert_eq!(
        result,
        Err(Ok(emergency_killswitch::Error::ContractPaused)),
        "do_transfer must fail while paused"
    );
}

// ---------------------------------------------------------------------------
// 2. Mutating functions fail while paused — do_mint
// ---------------------------------------------------------------------------
#[test]
fn test_pause_blocks_mint() {
    let (_, client, admin) = setup();

    client.pause(&admin);

    let minter = Address::generate(&client.env);
    let result = client.try_do_mint(&minter, &1000);
    assert_eq!(
        result,
        Err(Ok(emergency_killswitch::Error::ContractPaused)),
        "do_mint must fail while paused"
    );
}

// ---------------------------------------------------------------------------
// 3. Read-only functions still work while paused
// ---------------------------------------------------------------------------
#[test]
fn test_read_only_works_while_paused() {
    let (_, client, admin) = setup();

    client.pause(&admin);

    // is_paused returns the correct value
    assert!(client.is_paused(), "is_paused should return true");

    // get_admin resolves successfully
    let stored_admin = client.get_admin();
    assert_eq!(
        stored_admin,
        Some(admin.clone()),
        "get_admin must return the admin even while paused"
    );

    // get_scheduled_unpause doesn't panic when no schedule is set
    assert!(
        client.get_scheduled_unpause().is_none(),
        "no schedule should be set"
    );
}

// ---------------------------------------------------------------------------
// 4. Unpause restores mutating operations
// ---------------------------------------------------------------------------
#[test]
fn test_unpause_restores_operations() {
    let (_, client, admin) = setup();

    client.pause(&admin);
    client.unpause(&admin);

    assert!(!client.is_paused(), "contract should be unpaused");

    let user = Address::generate(&client.env);
    // should not panic / return error
    client.do_transfer(&user, &100);
    client.do_mint(&user, &200);
}

// ---------------------------------------------------------------------------
// 5. Non-admin cannot pause
// ---------------------------------------------------------------------------
#[test]
fn test_non_admin_cannot_pause() {
    let (env, client, _admin) = setup();
    env.mock_all_auths(); // already set by setup, kept explicit for clarity

    let rando = Address::generate(&env);
    let result = client.try_pause(&rando);
    assert_eq!(
        result,
        Err(Ok(emergency_killswitch::Error::Unauthorized)),
        "non-admin pause must be rejected"
    );
}

// ---------------------------------------------------------------------------
// 6. Non-admin cannot unpause
// ---------------------------------------------------------------------------
#[test]
fn test_non_admin_cannot_unpause() {
    let (env, client, admin) = setup();

    client.pause(&admin);

    let rando = Address::generate(&env);
    let result = client.try_unpause(&rando);
    assert_eq!(
        result,
        Err(Ok(emergency_killswitch::Error::Unauthorized)),
        "non-admin unpause must be rejected"
    );
}

// ---------------------------------------------------------------------------
// 7. Scheduled-unpause timestamp is enforced
// ---------------------------------------------------------------------------
#[test]
fn test_schedule_unpause_enforced() {
    let (env, client, admin) = setup();

    client.pause(&admin);

    // Set an unpause schedule 1 hour in the future.
    let now = env.ledger().timestamp();
    let future = now + 3600;
    client.schedule_unpause(&admin, &future);

    // Unpause attempt before the scheduled time must fail.
    let result = client.try_unpause(&admin);
    assert_eq!(
        result,
        Err(Ok(emergency_killswitch::Error::ContractPaused)),
        "unpause before scheduled time must be rejected"
    );

    // Advance ledger past the scheduled time and retry.
    env.ledger().set_timestamp(future + 1);
    client.unpause(&admin); // must succeed
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// 8. Admin transfer — new admin can pause; old admin cannot
// ---------------------------------------------------------------------------
#[test]
fn test_transfer_admin() {
    let (env, client, old_admin) = setup();

    let new_admin = Address::generate(&env);
    client.transfer_admin(&old_admin, &new_admin);

    // New admin can pause.
    client.pause(&new_admin);
    assert!(client.is_paused());

    // Old admin can no longer pause after transferring rights.
    client.unpause(&new_admin);
    let result = client.try_pause(&old_admin);
    assert_eq!(
        result,
        Err(Ok(emergency_killswitch::Error::Unauthorized)),
        "old admin must be rejected after transfer"
    );
}

// ---------------------------------------------------------------------------
// 9. Emergency pause emits an event
// ---------------------------------------------------------------------------
#[test]
fn test_emergency_pause_emits_event() {
    let (env, client, admin) = setup();

    client.pause(&admin);

    let events = env.events().all();
    assert!(!events.is_empty(), "at least one event should be emitted");

    // Find the "paused" event (last event emitted).
    let last = events.last().unwrap();
    let topics = &last.1;

    // Topic[0] = "killswitch", Topic[1] = "paused"
    let ns: Symbol = Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap();
    let action: Symbol = Symbol::try_from_val(&env, &topics.get(1).unwrap()).unwrap();

    use soroban_sdk::symbol_short;
    assert_eq!(ns, symbol_short!("killswtch"));
    assert_eq!(action, symbol_short!("paused"));
}
