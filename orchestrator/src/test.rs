#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as TestAddress, Env};

    fn create_test_env() -> Env {
        Env::default()
    }

    #[test]
    fn test_init_success() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let result = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        assert_eq!(result, Ok(true));

        // Verify stored addresses
        let stored_owner: Option<Address> =
            env.storage().instance().get(&symbol_short!("OWNER"));
        assert_eq!(stored_owner, Some(owner));
    }

    #[test]
    fn test_init_already_initialized() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        // First init should succeed
        let _result = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw.clone(),
            rs.clone(),
            sg.clone(),
            bp.clone(),
            ins.clone(),
        );

        // Second init should fail
        let result = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        assert_eq!(result, Err(OrchestratorError::Unauthorized));
    }

    #[test]
    fn test_init_duplicate_dependencies() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);

        // Pass same address for savings_goals and bill_payments
        let result = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg.clone(),
            sg, // Duplicate!
            bp,
        );

        assert_eq!(result, Err(OrchestratorError::DuplicateDependency));
    }

    #[test]
    fn test_init_self_reference() {
        let env = create_test_env();
        let caller = TestAddress::random(&env);
        let other = TestAddress::random(&env);
        let another = TestAddress::random(&env);
        let third = TestAddress::random(&env);
        let fourth = TestAddress::random(&env);

        // Pass caller as one of the dependencies
        let result = Orchestrator::init(
            env.clone(),
            caller.clone(),
            caller, // Self-reference!
            other,
            another,
            third,
            fourth,
        );

        assert_eq!(result, Err(OrchestratorError::DuplicateDependency));
    }

    #[test]
    fn test_get_nonce_uninitialized() {
        let env = create_test_env();
        let user = TestAddress::random(&env);

        let nonce = Orchestrator::get_nonce(env, user);
        assert_eq!(nonce, 0);
    }

    #[test]
    fn test_execute_flow_invalid_amount() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let request_hash = Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            0,
            0,
            deadline,
        );

        let result = Orchestrator::execute_remittance_flow(
            env,
            executor,
            0, // Invalid: amount must be > 0
            0,
            deadline,
            request_hash,
        );

        assert_eq!(result, Err(OrchestratorError::InvalidAmount));
    }

    #[test]
    fn test_execute_flow_invalid_nonce() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let request_hash = Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            5, // Wrong nonce (current is 0)
            100,
            deadline,
        );

        let result = Orchestrator::execute_remittance_flow(
            env,
            executor,
            100,
            5,
            deadline,
            request_hash,
        );

        assert_eq!(result, Err(OrchestratorError::InvalidNonce));
    }

    #[test]
    fn test_execute_flow_expired_deadline() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();
        let deadline = now - 100; // Past deadline
        let request_hash = Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            0,
            100,
            deadline,
        );

        let result = Orchestrator::execute_remittance_flow(
            env,
            executor,
            100,
            0,
            deadline,
            request_hash,
        );

        assert_eq!(result, Err(OrchestratorError::DeadlineExpired));
    }

    #[test]
    fn test_execute_flow_deadline_too_far_future() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();
        let deadline = now + MAX_DEADLINE_WINDOW_SECS + 1000; // Too far in future
        let request_hash = Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            0,
            100,
            deadline,
        );

        let result = Orchestrator::execute_remittance_flow(
            env,
            executor,
            100,
            0,
            deadline,
            request_hash,
        );

        assert_eq!(result, Err(OrchestratorError::DeadlineExpired));
    }

    #[test]
    fn test_execute_flow_invalid_hash() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();
        let deadline = now + 1000;

        let result = Orchestrator::execute_remittance_flow(
            env,
            executor,
            100,
            0,
            deadline,
            12345, // Wrong hash
        );

        assert_eq!(result, Err(OrchestratorError::InvalidNonce));
    }

    #[test]
    fn test_execute_flow_success_and_nonce_increment() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();
        let deadline = now + 1000;

        // Verify initial nonce is 0
        assert_eq!(Orchestrator::get_nonce(env.clone(), executor.clone()), 0);

        let request_hash = Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            0,
            100,
            deadline,
        );

        let result = Orchestrator::execute_remittance_flow(
            env.clone(),
            executor.clone(),
            100,
            0,
            deadline,
            request_hash,
        );

        assert_eq!(result, Ok(true));

        // Verify nonce incremented to 1
        assert_eq!(Orchestrator::get_nonce(env, executor), 1);
    }

    #[test]
    fn test_execute_flow_nonce_double_spend() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();
        let deadline = now + 1000;

        let request_hash = Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            0,
            100,
            deadline,
        );

        // First execution should succeed
        let _result = Orchestrator::execute_remittance_flow(
            env.clone(),
            executor.clone(),
            100,
            0,
            deadline,
            request_hash,
        );

        // Now nonce is 1. Try to use nonce 0 again (already used)
        let deadline2 = now + 2000;
        let request_hash2 = Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            0,
            100,
            deadline2,
        );

        let result2 = Orchestrator::execute_remittance_flow(
            env,
            executor,
            100,
            0, // Reusing nonce 0
            deadline2,
            request_hash2,
        );

        // Should fail: nonce already used
        assert_eq!(result2, Err(OrchestratorError::NonceAlreadyUsed));
    }

    #[test]
    fn test_get_execution_stats_initial() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let stats = Orchestrator::get_execution_stats(env);
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
    fn test_get_audit_log_empty() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let log = Orchestrator::get_audit_log(env, 0, 20);
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn test_get_audit_log_pagination() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();

        // Execute multiple flows to generate audit entries
        for i in 0..5 {
            let deadline = now + 1000 + i * 100;
            let request_hash = Orchestrator::compute_request_hash(
                symbol_short!("flow"),
                executor.clone(),
                i,
                100 + i as i128,
                deadline,
            );

            let _result = Orchestrator::execute_remittance_flow(
                env.clone(),
                executor.clone(),
                100 + i as i128,
                i,
                deadline,
                request_hash,
            );
        }

        // Get all audit entries
        let log = Orchestrator::get_audit_log(env, 0, 50);
        assert_eq!(log.len(), 5);
    }

    #[test]
    fn test_get_version() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let version = Orchestrator::get_version(env);
        assert_eq!(version, CONTRACT_VERSION);
    }

    #[test]
    fn test_set_version_success() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        env.budget().reset_unlimited();

        let result = Orchestrator::set_version(env.clone(), owner, 2);
        assert_eq!(result, Ok(true));

        let new_version = Orchestrator::get_version(env);
        assert_eq!(new_version, 2);
    }

    #[test]
    fn test_set_version_unauthorized() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        let non_owner = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let result = Orchestrator::set_version(env, non_owner, 2);
        assert_eq!(result, Err(OrchestratorError::Unauthorized));
    }

    #[test]
    fn test_reentrancy_lock() {
        let env = create_test_env();
        let owner = TestAddress::random(&env);
        let fw = TestAddress::random(&env);
        let rs = TestAddress::random(&env);
        let sg = TestAddress::random(&env);
        let bp = TestAddress::random(&env);
        let ins = TestAddress::random(&env);

        let _init = Orchestrator::init(
            env.clone(),
            owner.clone(),
            fw,
            rs,
            sg,
            bp,
            ins,
        );

        // Manually set execution lock (simulating reentrancy)
        env.storage()
            .instance()
            .set(&symbol_short!("EXEC_LOCK"), &true);

        let executor = TestAddress::random(&env);
        env.budget().reset_unlimited();

        let now = env.ledger().timestamp();
        let deadline = now + 1000;
        let request_hash = Orchestrator::compute_request_hash(
            symbol_short!("flow"),
            executor.clone(),
            0,
            100,
            deadline,
        );

        let result = Orchestrator::execute_remittance_flow(
            env,
            executor,
            100,
            0,
            deadline,
            request_hash,
        );

        assert_eq!(result, Err(OrchestratorError::ExecutionLocked));
    }
}
