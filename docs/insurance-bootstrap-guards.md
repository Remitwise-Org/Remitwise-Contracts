# Insurance Bootstrap Guards

This note documents the bootstrap safety contract for `insurance`.

## Initialization

`Insurance::init(owner)` is single-shot.

- First call: stores `Initialized = true`, records `Owner = owner`, resets
  `PolicyCount`, and creates an empty active-policy index.
- Later calls: return `InsuranceError::AlreadyInitialized`.

## Current authorization model

`init` does not currently call `require_auth`.

That means bootstrap safety relies on:

- deploying and initializing in a trusted flow
- the `AlreadyInitialized` guard preventing later ownership takeover

Once initialization succeeds, the stored owner becomes the only contract-level
admin for owner-only operations such as `set_external_ref`.

## Pre-init behavior

Before initialization:

- Mutators return `InsuranceError::NotInitialized`:
  - `create_policy`
  - `pay_premium`
  - `batch_pay_premiums`
  - `deactivate_policy`
  - `set_external_ref`
- Read paths are deterministic and non-panicking:
  - `get_policy` returns `Ok(None)`
  - `get_active_policies` returns an empty page
  - `get_total_monthly_premium` returns `Ok(0)`

## Post-init privileges

After initialization:

- policy owners may pay and deactivate their own policies
- the stored contract owner may perform owner-only admin updates
- a different address cannot re-run `init` or assume owner-only privileges
