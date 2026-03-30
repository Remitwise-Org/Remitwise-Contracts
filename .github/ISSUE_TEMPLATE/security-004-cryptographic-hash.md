---
name: "[SECURITY-004] Replace Checksum with Cryptographic Hash"
about: Use SHA-256 instead of simple checksum for data integrity
title: "[SECURITY-004] Replace Checksum with Cryptographic Hash"
labels: security, medium-priority, data-migration
assignees: ''
---

## Security Issue

**Severity:** MEDIUM
**Component:** data_migration module
**Threat ID:** T-DI-01

## Description

Data migration uses a simple checksum for integrity verification, which is vulnerable to collision attacks. An attacker could craft malicious data that produces the same checksum as legitimate data.

### Affected Functions
- `ExportSnapshot::compute_checksum()`
- `ExportSnapshot::verify_checksum()`
- `import_snapshot()` in all contracts

## Attack Scenario

1. Attacker exports legitimate snapshot
2. Modifies payload to inject malicious data
3. Crafts collision to match original checksum
4. Imports corrupted snapshot
5. Contract accepts corrupted data due to checksum match

## Proposed Solution

Replace simple checksum with SHA-256:

```rust
use sha2::{Sha256, Digest};

impl ExportSnapshot {
    pub fn compute_checksum(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(
            serde_json::to_vec(&self.payload)
                .expect("payload must be serializable")
        );
        hex::encode(hasher.finalize())
    }

    pub fn verify_checksum(&self) -> bool {
        self.header.checksum == self.compute_checksum()
    }
}
```

## Acceptance Criteria

- [ ] SHA-256 hash replaces simple checksum
- [ ] Backward compatibility maintained for existing snapshots
- [ ] Version field indicates hash algorithm used
- [ ] Tests verify collision resistance
- [ ] Performance impact measured
- [ ] Documentation updated with hash algorithm details

## Implementation Tasks

- [ ] Add sha2 dependency (already present)
- [ ] Replace checksum computation with SHA-256
- [ ] Add hash algorithm field to SnapshotHeader
- [ ] Implement backward compatibility for old checksums
- [ ] Write unit tests for hash verification
- [ ] Write tests for collision resistance
- [ ] Measure performance impact
- [ ] Update documentation
- [ ] Add migration guide for existing snapshots

## Testing Requirements

- Test SHA-256 hash computation
- Test hash verification success
- Test hash verification failure
- Test backward compatibility with old checksums
- Test performance vs simple checksum
- Attempt collision attack (should fail)

## Backward Compatibility

Support both old and new hash formats:

```rust
pub fn verify_checksum(&self) -> bool {
    match self.header.hash_algorithm.as_str() {
        "sha256" => self.header.checksum == self.compute_sha256(),
        "simple" => self.header.checksum == self.compute_simple_checksum(),
        _ => false,
    }
}
```

## Estimated Effort

1-2 days

## Related Issues

- Relates to THREAT_MODEL.md Section 3.5
- Should be completed before mainnet deployment
