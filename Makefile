.PHONY: wasm

# Builds every contract's release WASM in one command, matching what CI's
# "Build workspace (WASM)" step does -- see .github/workflows/ci.yml.
wasm:
	cargo build --release --target wasm32-unknown-unknown
	@echo "WASM output: target/wasm32-unknown-unknown/release/*.wasm"
