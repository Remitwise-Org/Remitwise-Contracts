pub fn pay_premium(env: Env, policy_id: BytesN<32>) {
    let killswitch_id = get_killswitch_id(&env);
    let is_paused: bool = env.invoke_contract(&killswitch_id, &symbol_short!("is_paused"), vec![&env, Symbol::new(&env, "insurance")].into());
    
    if is_paused {
        panic!("Contract is currently paused for emergency maintenance.");
    }
    // ... rest of the logic
}