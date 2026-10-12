.PHONY: help build release check fmt fmt-check lint test test-core test-cli test-mcp \
        audit smoke run mcp install uninstall clean doc tree deps-update

CARGO ?= cargo
BIN   ?= clickupc
PREFIX ?= /usr/local

help:
	@echo "clickupc — common workspace targets"
	@echo ""
	@echo "  make build           Debug build (workspace)"
	@echo "  make release         Release build (workspace)"
	@echo "  make install         Install clickupc to \$$PREFIX/bin"
	@echo "  make fmt | fmt-check | lint | test | test-core | test-cli | test-mcp"
	@echo "  make audit           cargo audit"
	@echo "  make smoke           fmt-check + lint + test + build (matches CI gate)"
	@echo "  make run ARGS=...    Run clickupc with ARGS"
	@echo "  make mcp             Run the MCP server (stdio)"

build:
	$(CARGO) build --workspace --all-targets

release:
	$(CARGO) build --workspace --release

check:
	$(CARGO) check --workspace --all-targets --all-features

fmt:
	$(CARGO) fmt --all

fmt-check:
	$(CARGO) fmt --all -- --check

lint:
	$(CARGO) clippy --workspace --all-targets --all-features -- -D warnings

test: test-core test-cli test-mcp

test-core:
	$(CARGO) test -p clickupc-core

test-cli:
	$(CARGO) test -p clickupc

test-mcp:
	$(CARGO) test -p clickupc-mcp

audit:
	$(CARGO) audit

smoke: fmt-check lint test
	$(CARGO) build --all
	@echo "Smoke OK."

run:
	$(CARGO) run -p clickupc -- $(ARGS)

mcp:
	$(CARGO) run -p clickupc -- mcp serve

install: release
	install -m 0755 target/release/$(BIN) $(PREFIX)/bin/$(BIN)

uninstall:
	rm -f $(PREFIX)/bin/$(BIN)

clean:
	$(CARGO) clean

doc:
	$(CARGO) doc --workspace --no-deps

tree:
	$(CARGO) tree --workspace --depth 1

deps-update:
	$(CARGO) update
