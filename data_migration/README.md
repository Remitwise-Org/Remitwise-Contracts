# Data Migration

Utility library for exporting and importing contract state snapshots for RemitWise.

## Features

- **Multi-format Support**: JSON, Binary (Bincode), CSV, and Encrypted.
- **Integrity Checks**: SHA-256 checksum validation for all snapshots.
- **Version Compatibility**: Enforces semantic versioning rules for snapshots (`MIN_SUPPORTED_VERSION`).
- **Replay Protection**: The `MigrationTracker` prevents duplicate/repeated imports of the same snapshot by tracking checksum/version pairs.

## Replay Protection

To prevent accidental double-restores or replaying of old data, all import functions (`import_from_json`, `import_from_binary`) require a `MigrationTracker`.

```rust
use data_migration::{MigrationTracker, import_from_json};

let mut tracker = MigrationTracker::new();
let result = import_from_json(&mut tracker, json_data);

if let Err(MigrationError::DuplicateImport) = result {
    println!("This snapshot has already been processed!");
}
```

The tracker should be persisted or maintained throughout the lifecycle of the migration process to ensure consistent protection.
