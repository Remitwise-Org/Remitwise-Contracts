# Contributor Overview

Welcome to **RemitWise Contracts**! This guide helps you get productive on day one.

## Getting Started

1. Install Rust stable and the Soroban SDK:
```bash
rustup toolchain install stable
cargo install --locked soroban-cli
```

2. Build the contracts for WASM:
```bash
cargo build --release --target wasm32-unknown-unknown
```

3. Run unit tests:
```bash
cargo test
```

## Example: Adding a New Entry Point

To add a new contract function:
```rust
#[contractimpl]
pub struct MyContract;

#[contractimpl]
impl MyContract {
    pub fn hello(env: Env, name: Symbol) -> Symbol {
        let greeting = Symbol::new(&env, "Hello");
        env.log().debug(&greeting);
        greeting.concat(&name)
    }
}
```

Build and test the function locally:
```bash
cargo test -p my_contract -- --nocapture
```

## Contributing Workflow

- Fork the repository.
- Create a feature branch: `git checkout -b feat/your-feature`.
- Make changes, ensure `cargo fmt`, `cargo clippy -- -D warnings`, and tests pass.
- Open a PR targeting `main` with a clear description. The PR template includes a checklist.

## Useful Commands

- `cargo fmt` – format code.
- `cargo clippy -- -D warnings` – lint.
- `cargo test` – run all tests.
- `cargo test -p <crate>` – run tests for a specific crate.

## Links

- Repository: https://github.com/your-org/remitwise-contracts
- CI: https://github.com/your-org/remitwise-contracts/actions
