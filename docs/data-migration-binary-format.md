**Binary Format Stability**

This document guarantees that the `data_migration` crate's binary
serialization format is stable and deterministic across builds of the
same version. In particular:

- Exporting the same `ExportSnapshot` twice must yield byte-identical
  output. This ensures checksums and repository-stored backups remain
  verifiable.
- A frozen golden vector is checked into `data_migration/tests/` to
  provide regression protection: the golden vector must continue to
  import to the expected `SnapshotPayload` across releases.

Notes:

- The exported checksum binds the schema version and format label in
  addition to the canonical JSON payload; changing the payload
  canonicalisation or the checksum algorithm will break the
  compatibility guarantees and must be coordinated via a major
  version bump and migration tooling.
- Tests in `data_migration/src/lib.rs` exercise round-trip stability,
  determinism, and size-bound rejections.
