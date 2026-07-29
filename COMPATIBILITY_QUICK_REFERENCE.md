# Compatibility Quick Reference

Quick reference for Soroban and Stellar version compatibility.

## Current Versions (Production)

```
Soroban SDK:     21.0.0
Soroban CLI:     21.0.0
Rust:            stable (latest)
Protocol:        20+
Targets:         wasm32-unknown-unknown, wasm32v1-none
```

## Installation

```bash
# Install Soroban CLI
cargo install --locked --version 21.0.0 soroban-cli

# Add Rust targets
rustup target add wasm32-unknown-unknown wasm32v1-none

# Verify
soroban version
rustc --version
```

## Quick Compatibility Check

```bash
# Check your versions
soroban version          # Should be 21.0.0
rustc --version          # Should be stable
cargo --version          # Should be latest

# Check network protocol
curl -X POST https://soroban-testnet.stellar.org \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"getNetwork","params":[]}'
```

## Build & Test

```bash
# Clean build
cargo clean
cargo build --release --target wasm32-unknown-unknown

# Run tests
cargo test

# Gas benchmarks
./scripts/run_gas_benchmarks.sh
```

## Upgrade Quick Steps

```bash
# 1. Update dependencies (all Cargo.toml files)
find . -name "Cargo.toml" -type f -exec sed -i 's/soroban-sdk = "OLD"/soroban-sdk = "NEW"/g' {} +

# 2. Update CLI
cargo install --locked --version X.Y.Z soroban-cli

# 3. Clean & rebuild
cargo clean && cargo build --release --target wasm32-unknown-unknown

# 4. Test
cargo test

# 5. Deploy to testnet
soroban contract deploy --wasm <path> --source <key> --network testnet
```

## Troubleshooting

### Build fails
```bash
cargo clean
rm -rf target/ Cargo.lock
cargo build --release --target wasm32-unknown-unknown
```

### Tests fail
```bash
# Check versions match
grep "soroban-sdk" */Cargo.toml

# Ensure CLI matches SDK
soroban version
```

### Deployment fails
```bash
# Verify network
soroban config network ls

# Check account balance
soroban keys address <identity>
```

## Status Indicators

- ✅ Tested and production ready
- ⚠️ Untested, validation required
- ❌ Known issues or incompatible

## More Information

- Full details: [README.md#compatibility](README.md#compatibility)
- Upgrade guide: [UPGRADE_GUIDE.md](UPGRADE_GUIDE.md)
- Version matrix: [VERSION_COMPATIBILITY.md](VERSION_COMPATIBILITY.md)
- Deployment: [DEPLOYMENT.md](DEPLOYMENT.md)

## Support

- [Soroban Docs](https://soroban.stellar.org/docs)
- [Discord](https://discord.gg/stellar)
- [Stack Exchange](https://stellar.stackexchange.com/)
