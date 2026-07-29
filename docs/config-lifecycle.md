# Config Lifecycle

This document explains how configuration keys are managed (added, deprecated, removed) in the Remitwise-Contracts ecosystem.

## Audience: Downstream Integrator

When integrating with our contracts, you may interact with configuration keys (e.g., limits, feature toggles).

### Adding a new config key

When a new config key is added, it will be announced in the release notes. The new key is available immediately upon upgrade.
Example: If we add a `MAX_FEE_BPS` key, it will be available via the `get_config` endpoint.

### Deprecating a config key

Keys marked for deprecation will emit a `DeprecationWarning` event when accessed. Integrators should migrate to the replacement key within the deprecation period (usually 2 minor releases).

### Removing a config key

Removed keys will return a `KeyNotFound` error. Ensure you have migrated to the new key before the removal release.
