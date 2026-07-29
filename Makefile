CONTRACTS := remittance_split savings_goals bill_payments insurance
WASM_DIR := target/wasm32-unknown-unknown/release
BINDINGS_DIR := bindings

.PHONY: wasm bindings build

# Builds every contract's release WASM in one command, matching what CI's
# "Build workspace (WASM)" step does -- see .github/workflows/ci.yml.
wasm:
	cargo build --release --target wasm32-unknown-unknown
	@echo "WASM output: $(WASM_DIR)/*.wasm"

# Regenerates the TypeScript client bindings for every contract from its
# just-built WASM. Depends on `wasm` so bindings are always regenerated
# from current source rather than read from a stale WASM file left over
# from a previous build.
bindings: wasm
	@for contract in $(CONTRACTS); do \
		echo "Generating bindings for $$contract..."; \
		stellar contract bindings typescript \
			--wasm $(WASM_DIR)/$$contract.wasm \
			--output-dir $(BINDINGS_DIR)/$$contract \
			--overwrite; \
	done

# One-command build: WASM plus its bindings, so bindings/ can't drift out
# of sync with the contracts it was generated from.
build: bindings
