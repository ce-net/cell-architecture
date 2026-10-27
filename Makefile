CELLS ?= .
CARGO ?= cargo

.PHONY: check test

check:            ## the architecture rule over every cell under CELLS (default: this directory)
	$(CARGO) run --quiet --manifest-path check/Cargo.toml -- $(CELLS)

test:             ## the checker's own tests, including the negative control
	$(CARGO) test --quiet --manifest-path check/Cargo.toml
