FRONTEND := frontend

# Cargo decides where artifacts go: CARGO_TARGET_DIR, build.target-dir in
# ~/.cargo/config.toml, or ./target. Ask it instead of hardcoding `target/`, so a
# global target directory (and CARGO_BUILD_TARGET) works. Override on the command
# line with `make TARGET_DIR=/path ...` if needed.
TARGET_DIR ?= $(shell cargo metadata --format-version 1 --no-deps 2>/dev/null \
	| sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
TARGET_DIR := $(if $(TARGET_DIR),$(TARGET_DIR),target)
TARGET_TRIPLE ?= $(CARGO_BUILD_TARGET)
RELEASE_DIR := $(TARGET_DIR)$(if $(TARGET_TRIPLE),/$(TARGET_TRIPLE),)/release
BIN         := $(RELEASE_DIR)/bia-quiz
IMPORT_BIN  := $(RELEASE_DIR)/bia-import

.PHONY: all build artefacts build-artefact cross frontend import run test check clean distclean doctor uname

all: build

frontend:
	pnpm --dir $(FRONTEND) install
	pnpm --dir $(FRONTEND) build

# The release binary embeds frontend/dist at compile time; figures live in SQLite.
build: frontend
	cargo build --release

# Host-independent Linux x86_64 release build (works from macOS or aarch64 Linux).
# Requires: rustup target add x86_64-unknown-linux-gnu
#           cargo install cargo-zigbuild
#           Zig in PATH (brew install zig, or your package manager)
# The default glibc is older than Debian 12/13 so the binary runs on both.
CROSS_TARGET ?= x86_64-unknown-linux-gnu
CROSS_GLIBC  ?= 2.36
CROSS_DIR    := $(TARGET_DIR)/$(CROSS_TARGET)/release
CROSS_BIN    := $(CROSS_DIR)/bia-quiz
CROSS_IMPORT := $(CROSS_DIR)/bia-import
ARTEFACT_DIR := deploy/artefacts
APP_CONFIG   ?= deploy/bia.toml

artefacts: frontend
	@command -v cargo-zigbuild >/dev/null 2>&1 || { \
		echo "cargo-zigbuild is missing: cargo install cargo-zigbuild" >&2; exit 1; }
	@command -v zig >/dev/null 2>&1 || { \
		echo "zig is missing: install Zig (e.g. brew install zig)" >&2; exit 1; }
	cargo zigbuild --release --target $(CROSS_TARGET).$(CROSS_GLIBC) \
		-p bia-server --bin bia-quiz
	cargo zigbuild --release --target $(CROSS_TARGET).$(CROSS_GLIBC) \
		-p bia-importer --bin bia-import
	mkdir -p $(ARTEFACT_DIR)
	rm -f $(ARTEFACT_DIR)/bia-quiz.service
	cp $(CROSS_BIN) $(ARTEFACT_DIR)/bia-quiz
	cp $(CROSS_IMPORT) $(ARTEFACT_DIR)/bia-import
	cp deploy/bia.toml.example $(ARTEFACT_DIR)/bia.toml.example
	@if [ -f "$(APP_CONFIG)" ]; then \
		cp "$(APP_CONFIG)" $(ARTEFACT_DIR)/bia.toml; \
		chmod 0600 $(ARTEFACT_DIR)/bia.toml; \
		echo "staged real config from $(APP_CONFIG)"; \
	elif [ ! -f $(ARTEFACT_DIR)/bia.toml ]; then \
		echo "no real config staged; create deploy/bia.toml or $(ARTEFACT_DIR)/bia.toml"; \
	fi
	chmod 0755 $(ARTEFACT_DIR)/bia-quiz $(ARTEFACT_DIR)/bia-import
	chmod 0644 $(ARTEFACT_DIR)/bia.toml.example
	file $(ARTEFACT_DIR)/bia-quiz $(ARTEFACT_DIR)/bia-import
	@echo "artefacts staged in $(ARTEFACT_DIR)/"

# Canonical data lives upstream (plan.md §10). The importer downloads the TSV and
# figures (Rust HTTP + tar/gzip), then loads SQLite.
UPSTREAM ?= https://github.com/Villemarque/bia-annales-database-csv/archive/refs/heads/master.tar.gz

import: build
	"$(IMPORT_BIN)" \
		--fetch \
		--upstream "$(UPSTREAM)" \
		--tsv annales-bia.tsv \
		--db bia.sqlite3 \
		--images data/figures \
		--report import-report.txt

run: build
	BIA_QUIZ_PROF_PASSWORD=$${BIA_QUIZ_PROF_PASSWORD:-dev} \
	BIA_QUIZ_ADMIN_PASSWORD=$${BIA_QUIZ_ADMIN_PASSWORD:-dev} "$(BIN)"

test:
	cargo test --workspace
	pnpm --dir $(FRONTEND) check

check:
	cargo fmt --check
	cargo clippy --workspace --all-targets -- -D warnings
	pnpm --dir $(FRONTEND) check

uname:
	@uname -a

doctor:
	@echo "host OS/arch       : $$(uname -s) $$(uname -m)"
	@echo "rustc              : $$(command -v rustc >/dev/null 2>&1 && rustc --version || echo missing)"
	@echo "cargo              : $$(command -v cargo >/dev/null 2>&1 && cargo --version || echo missing)"
	@echo "rustc host triple  : $$(rustc -vV 2>/dev/null | sed -n 's/^host: //p')"
	@echo "CARGO_TARGET_DIR   : $${CARGO_TARGET_DIR:-<unset>}"
	@echo "CARGO_BUILD_TARGET : $${CARGO_BUILD_TARGET:-<unset>}"
	@echo "target dir         : $(TARGET_DIR)"
	@echo "target triple      : $(if $(TARGET_TRIPLE),$(TARGET_TRIPLE),<host>)"
	@echo "release dir        : $(RELEASE_DIR)"
	@echo "server binary      : $(BIN)"
	@echo "importer binary    : $(IMPORT_BIN)"
	@echo "cross binary       : $(CROSS_BIN)"
	@echo "artefacts dir      : $(ARTEFACT_DIR)"
	@echo "frontend build     : $$(test -f $(FRONTEND)/dist/index.html && echo present || echo missing)"
	@echo "figures            : $$(ls data/figures 2>/dev/null | wc -l | tr -d ' ') files"
	@if [ -x "$(BIN)" ]; then \
		echo "binary file        : $$(file -b "$(BIN)")"; \
		echo "binary size        : $$(du -h "$(BIN)" | cut -f1)"; \
		if command -v readelf >/dev/null 2>&1; then \
			echo "interpreter        : $$(readelf -l "$(BIN)" 2>/dev/null | awk '/interpreter/{print $$NF}')"; \
		fi; \
	else \
		echo "binary status      : missing (run make build)"; \
	fi

# Build artifacts only: keeps bia.sqlite3 (and therefore the stored results).
# With a shared CARGO_TARGET_DIR, only this workspace's packages are cleaned;
# dependencies and other projects stay cached. Use `cargo clean` for everything.
clean:
	cargo clean -p bia-core -p bia-server -p bia-importer
	rm -rf $(FRONTEND)/dist $(FRONTEND)/node_modules .pnpm-store
	rm -f $(ARTEFACT_DIR)/bia-quiz $(ARTEFACT_DIR)/bia-import \
		$(ARTEFACT_DIR)/bia-quiz.service $(ARTEFACT_DIR)/bia.toml.example

# Everything, including the imported database and its report. Destructive.
distclean: clean
	rm -f bia.sqlite3 bia.sqlite3-shm bia.sqlite3-wal import-report.txt
