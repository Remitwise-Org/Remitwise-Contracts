#[cfg(test)]
mod tests {
    use crate::{
        ExecutionStats, Orchestrator, OrchestratorClient, OrchestratorError,
        MAX_DEADLINE_WINDOW_SECS,
    };
    use remitwise_common::CONTRACT_VERSION;
    use soroban_sdk::{
        symbol_short,
        testutils::{Address as _, Ledger as _},
        Address, Env, Symbol,
    };

    fn setup_test() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(100_000);
        let owner = Address::generate(&env);
        (env, owner)
    }

    fn register_orchestrator(env: &Env) -> OrchestratorClient<'_> {
        let contract_id = env.register_contract(None, Orchestrator);
        OrchestratorClient::new(env, &contract_id)
    }

    fn compute_test_hash(
        _env: &Env,
        operation: Symbol,
        nonce: u64,
        amount: i128,
        deadline: u64,
    ) -> u64 {
        let op_bits: u64 = operation.to_val().get_payload();
        let amt_lo = amount as u64;
        let amt_hi = (amount >> 64) as u64;

        op_bits
            .wrapping_add(nonce)
            .wrapping_add(amt_lo)
            .wrapping_add(amt_hi)
            .wrapping_add(deadline)
            .wrapping_mul(1_000_000_007)
    }

    fn init_orchestrator(env: &Env, client: &OrchestratorClient, owner: &Address) {
        let fw = Address::generate(env);
        let rs = Address::generate(env);
        let sg = Address::generate(env);
        let bp = Address::generate(env);
        let ins = Address::generate(env);

        client.init(owner, &fw, &rs, &sg, &bp, &ins);
    }

    #[test]
    fn test_init_success() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        let fw = Address::generate(&env);
        let rs = Address::generate(&env);
        let sg = Address::generate(&env);
        let bp = Address::generate(&env);
        let ins = Address::generate(&env);

        let result = client.try_init(&owner, &fw, &rs, &sg, &bp, &ins);

        assert_eq!(result, Ok(Ok(true)));
    }

    #[test]
    fn test_init_already_initialized() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        let result = client.try_init(
            &owner,
            &Address::generate(&env),
            &Address::generate(&env),
            &Address::generate(&env),
            &Address::generate(&env),
            &Address::generate(&env),
        );

        assert_eq!(result, Err(Ok(OrchestratorError::Unauthorized)));
    }

    #[test]
    fn test_get_nonce() {
        let (env, _owner) = setup_test();
        let client = register_orchestrator(&env);
        let user = Address::generate(&env);
        assert_eq!(client.get_nonce(&user), 0);
    }

    #[test]
    fn test_get_version() {
        let (env, _owner) = setup_test();
        let client = register_orchestrator(&env);
        assert_eq!(client.get_version(), CONTRACT_VERSION);
    }

    #[test]
    fn test_set_version_success() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        client.set_version(&owner, &2);
        assert_eq!(client.get_version(), 2);
    }

    #[test]
    fn test_set_version_unauthorized() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        let non_owner = Address::generate(&env);
        let result = client.try_set_version(&non_owner, &2);
        assert_eq!(result, Err(Ok(OrchestratorError::Unauthorized)));
    }

    #[test]
    fn test_execute_flow_invalid_amount() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        let executor = Address::generate(&env);
        let deadline = env.ledger().timestamp() + 1000;

        let hash = compute_test_hash(
            &env,
            symbol_short!("flow"),
            0,
            0, // Invalid amount
            deadline,
        );

        let result = client.try_execute_remittance_flow(
            &executor, &0, // amount 0
            &0, &deadline, &hash,
        );

        assert_eq!(result, Err(Ok(OrchestratorError::InvalidAmount)));
    }

    #[test]
    fn test_execute_flow_expired_deadline() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        let executor = Address::generate(&env);
        let deadline = env.ledger().timestamp() - 100; // Expired

        let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

        let result = client.try_execute_remittance_flow(&executor, &1000, &0, &deadline, &hash);

        assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));
    }

    #[test]
    fn test_execute_flow_deadline_too_far() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        let executor = Address::generate(&env);
        let deadline = env.ledger().timestamp() + MAX_DEADLINE_WINDOW_SECS + 1000;

        let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

        let result = client.try_execute_remittance_flow(&executor, &1000, &0, &deadline, &hash);

        assert_eq!(result, Err(Ok(OrchestratorError::DeadlineExpired)));
    }

    #[test]
    fn test_execute_flow_invalid_hash() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        let executor = Address::generate(&env);
        let deadline = env.ledger().timestamp() + 1000;

        let bad_hash = 12345u64;

        let result = client.try_execute_remittance_flow(&executor, &1000, &0, &deadline, &bad_hash);

        assert_eq!(result, Err(Ok(OrchestratorError::InvalidNonce)));
    }

    #[test]
    fn test_get_execution_stats_initial() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        let stats = client.get_execution_stats();
        assert_eq!(
            stats,
            Some(ExecutionStats {
                total_executions: 0,
                successful_executions: 0,
                failed_executions: 0,
                last_execution_time: 0,
            })
        );
    }

    #[test]
    fn test_reentrancy_lock() {
        let (env, owner) = setup_test();
        let client = register_orchestrator(&env);
        init_orchestrator(&env, &client, &owner);

        // Manually set execution lock (simulating reentrancy)
        env.as_contract(&client.address, || {
            env.storage()
                .instance()
                .set(&symbol_short!("EXEC_LOCK"), &true);
        });

        let executor = Address::generate(&env);
        let deadline = env.ledger().timestamp() + 1000;
        let hash = compute_test_hash(&env, symbol_short!("flow"), 0, 1000, deadline);

        let result = client.try_execute_remittance_flow(&executor, &1000, &0, &deadline, &hash);

        assert_eq!(result, Err(Ok(OrchestratorError::ExecutionLocked)));
    }
}
