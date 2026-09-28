# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

.PHONY: all ci deploy publish rust build version install link clean submodules check test lint fmt-html fmt headers headers-check authors up reload down logs

$(eval $(shell ./scripts/ci_wrapper.sh --env 2>/dev/null))

VERSION := $(shell git describe --tags --match 'v*' --abbrev=0 2>/dev/null | sed 's/^v//')
export VERSION

ROOT_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
BUILD_DIR := $(ROOT_DIR)/build
export ROOT_DIR BUILD_DIR

all: build

ci:
	@echo "══════════════════════════════════════════"
	@echo "  CI Environment"
	@echo "══════════════════════════════════════════"
	@echo "  CI_SYSTEM:       $(CI_SYSTEM)"
	@echo "  CI_EVENT:        $(CI_EVENT)"
	@echo "  CI_EVENT_PATH:   $(CI_EVENT_PATH)"
	@echo "  CI_STATE:        $(CI_STATE)"
	@echo "  CI_REF:          $(CI_REF)"
	@echo "  CI_BASE_REF:     $(CI_BASE_REF)"
	@echo "  CI_HEAD_REF:     $(CI_HEAD_REF)"
	@echo "  CI_COMMIT:       $(CI_COMMIT)"
	@echo "  CI_COMMIT_SHORT: $(CI_COMMIT_SHORT)"
	@echo "  CI_MSG:          $(CI_MSG)"
	@echo "  CI_BRANCH:       $(CI_BRANCH)"
	@echo "  CI_TARGET:       $(CI_TARGET)"
	@echo "  CI_ENV:          $(CI_ENV)"
	@echo "  CI_TRACK:        $(CI_TRACK)"
	@echo "  CI_TAG:          $(CI_TAG)"
	@echo "  CI_OWNER:        $(CI_OWNER)"
	@echo "  CI_REPO:         $(CI_REPO)"
	@echo "  CI_SLUG:         $(CI_SLUG)"
	@echo "  CI_URL:          $(CI_URL)"
	@echo "  CI_CHANGE:       $(CI_CHANGE)"
	@echo "  CI_RUN:          $(CI_RUN)"
	@echo "  CI_RUN_URL:      $(CI_RUN_URL)"
	@echo "  CI_ACTOR:        $(CI_ACTOR)"
	@echo "  CI_EMAIL:        $(CI_EMAIL)"
	@echo "  CI_PIPELINE:     $(CI_PIPELINE)"
	@echo "  CI_JOB:          $(CI_JOB)"
	@echo "  CI_TIMESTAMP:    $(CI_TIMESTAMP)"
	@echo "  CI_WORKSPACE:    $(CI_WORKSPACE)"
	@echo "  CI_DIR:          $(CI_DIR)"
	@echo "  CI_PARENT:       $(CI_PARENT)"
	@echo "══════════════════════════════════════════"

deploy:
	@echo "Deploying $(CI_COMMIT_SHORT) from $(CI_BRANCH) [env=$(CI_ENV) track=$(CI_TRACK)]..."

version:
	@echo $(VERSION)

# Local stack (deploy/compose.yml); see scripts/dev.sh.
up reload down logs:
	@./scripts/dev.sh $@

# One cargo run builds every Rust crate in parallel with shared dependencies,
# so the per-crate builds that each_lib.sh triggers find their binaries fresh.
rust:
	@cargo build --workspace --profile release

build: rust
	@MAKE=$(MAKE) ./scripts/each_lib.sh $@

# install only copies what build staged, so it runs under sudo without cargo.
install:
	@MAKE=$(MAKE) ./scripts/each_lib.sh $@

link: rust
	@MAKE=$(MAKE) ./scripts/each_lib.sh $@

publish: rust
	@MAKE=$(MAKE) ./scripts/each_lib.sh $@

submodules:
	git submodule update --init --recursive

clean:
	@rm -rf $(BUILD_DIR)
	@MAKE=$(MAKE) ./scripts/each_lib.sh $@
	@cargo clean

# The Rust crates under lib/ form one cargo workspace (see Cargo.toml).
check: lint test
	@cargo fmt --all --check

test:
	@cargo test --workspace

lint:
	@cargo clippy --workspace --all-targets -- -D warnings

fmt-html:
	@if ! command -v djlint >/dev/null 2>&1; then \
		echo "Installing djlint..."; \
		uv tool install djlint; \
	fi
	@echo "Formatting HTML templates with djlint..."; \
	djlint --extension=jinja2 --reformat "lib/api/templates" --indent 2 || true

fmt: fmt-html
	@cargo fmt --all

HGL := target/release/uc-repo-headers

headers:
	@$(MAKE) -s -C lib/hgl build
	@$(HGL) fix

headers-check:
	@$(MAKE) -s -C lib/hgl build
	@$(HGL) check

UC_REPO := target/x86_64-unknown-linux-musl/release/uc-repo

authors:
	@$(MAKE) -s -C lib/uc build
	@$(UC_REPO) authors
