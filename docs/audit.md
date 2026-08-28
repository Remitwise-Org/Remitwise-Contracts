# Event Emission Audit Findings

During the implementation of the `RemitwiseEvents::emit` size guard, we audited the workspace for any emit calls passing large structs or `Vec`s as data. The maximum recommended size for serialized event data is 256 bytes. 

## Findings

1. **`remittance_split/src/lib.rs`**: Emits the `SplitCalculated` event which contains a distribution mapping. While typically small, this could exceed the 256-byte limit if there are many payees in the distribution.
2. **`bill_payments/src/lib.rs`**: The `BillCreated` event emits bill details. Depending on the length of strings in the bill struct, this could occasionally approach the boundary.
3. **`savings_goals/src/lib.rs`**: The `GoalCreated` event contains goal parameters. This usually stays under 256 bytes but should be monitored.
4. **`family_wallet/src/lib.rs`**: Emits `TransactionProposed` with transaction details.

**Conclusion**: Most payloads are IDs or small state updates. However, the events that emit structs representing new entities (e.g., bills, goals) or distributions (e.g., `SplitCalculated`) are at risk of exceeding the new budget if they contain large string names or vectors of data. The newly introduced guard will catch these in testing.
