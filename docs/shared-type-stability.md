# Shared-Type Stability Guarantee

## Overview

`remitwise-common` defines three `#[contracttype] #[repr(u32)]` enums that are shared across all Remitwise contracts:

| Type           | Variants                                            | Used in                                                                    |
| -------------- | --------------------------------------------------- | -------------------------------------------------------------------------- |
| `Category`     | `Spending=1, Savings=2, Bills=3, Insurance=4`       | remittance_split split percentages, reporting                              |
| `CoverageType` | `Health=1, Life=2, Property=3, Auto=4, Liability=5` | insurance `PolicyCreatedEvent.coverage_type`, policy storage               |
| `FamilyRole`   | `Owner=1, Admin=2, Member=3, Viewer=4`              | family_wallet member records, multisig proposals, role-change transactions |

## Why Stability Matters

These types are written to Soroban **persistent/instance storage** and emitted in **event payloads** (e.g. `PolicyCreatedEvent.coverage_type`). Because Soroban encodes `#[contracttype]` enums by their discriminant value, any renumbering silently corrupts:

- Data already stored on-chain (reads back as the wrong variant)
- Cross-contract calls that pass these types as arguments
- Off-chain indexer rows that decode event payloads

## Stability Guarantee

The discriminant values are **pinned** by two independent test layers in `remitwise-common/src/lib.rs`:

1. **Discriminant tests** (`category_discriminants`, `coverage_type_discriminants`, `family_role_discriminants`) — assert the raw `as u32` value of every variant.

2. **Round-trip tests** (`*_roundtrip`, `*_all_variants_roundtrip`, `*_roundtrip_preserves_discriminant`) — encode each variant through `IntoVal<Env, Val>` and decode it back with `TryFromVal<Env, Val>`, asserting the decoded value equals the original and that the discriminant is unchanged.

Both layers must pass in CI before any change to these types can be merged.

## Adding a New Variant

If you need to add a new variant:

1. **Append only** — assign the next sequential discriminant (e.g. `Category::Investment = 5`). Never reuse or reorder existing values.
2. Update the discriminant test to include the new variant.
3. Add a round-trip test for the new variant.
4. Update any off-chain indexer code that switches on the enum value.
5. Document the addition in `CHANGELOG_CONTRACTS.md`.

## Removing or Renaming a Variant

Removing or renaming a variant is a **breaking change**. It requires:

1. A contract migration path for any on-chain data that stores the old variant.
2. A version bump in `CONTRACT_VERSION`.
3. Coordination with all indexer consumers.

## References

- `remitwise-common/src/lib.rs` — enum definitions and tests
- `insurance/src/lib.rs` — `CoverageType` usage in `PolicyCreatedEvent`
- `family_wallet/src/lib.rs` — `FamilyRole` usage in `FamilyMember` and `TransactionData::RoleChange`
- `remittance_split/src/lib.rs` — `Category` usage in split configuration
