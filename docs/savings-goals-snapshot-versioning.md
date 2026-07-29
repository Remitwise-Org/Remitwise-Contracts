# Savings Goals Snapshot Versioning

## Supported Range
Snapshots imported via `import_snapshot` must strictly fall within the following version bounds:
* **Minimum Supported Version:** `MIN_SUPPORTED_SCHEMA_VERSION`
* **Maximum Supported Version:** `SCHEMA_VERSION`

## Validation Sequence
To preserve smart contract integrity and prevent partial data corruption, imports enforce a strict validate-before-apply order:
1. **Schema Version Check:** Fails immediately with `UnsupportedVersion` if the snapshot version falls outside valid boundaries. No data configuration or storage allocation occurs.
2. **Checksum Verification:** Validates snapshot data consistency, throwing a `ChecksumMismatch` if the payload has been tampered with or modified.
3.