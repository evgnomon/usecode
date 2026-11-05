# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := usecode-agent-api
BIN := uc-agent-api
# The old name, kept for a release so scripts calling it carry on working.
ALIASES := usecode-agent-api
EXTRA_SOURCES := $(shell find migrations templates static model_image testdata -type f) build.rs
TEST_DB_PORT ?= 55433
# The database tests run against it while `make test-db` is up, and skip
# themselves when it isn't.
export USECODE_AGENT_TEST_DATABASE_URL ?= postgres://postgres:test@127.0.0.1:$(TEST_DB_PORT)/api

include ../rust.mk

.PHONY: run migrate migrate-status test-db test-db-stop push rollout deploy

run:
	@cargo run -p $(NAME)

# Bring every database in USECODE_AGENT_DATABASE_URL and _SHARDS up to date.
migrate:
	@cargo run -q -p $(NAME) -- migrate

migrate-status:
	@cargo run -q -p $(NAME) -- migrate --status

test-db:
	@./scripts/test-db.sh start $(TEST_DB_PORT)

test-db-stop:
	@./scripts/test-db.sh stop

# Builds the latest push (origin/master) into an image and pushes it to the
# cluster's registry; see scripts/deploy.sh.
push:
	@./scripts/deploy.sh push $(REGISTRY)

# Points console/api at the latest push and checks it answers.
rollout:
	@./scripts/deploy.sh rollout $(REGISTRY)

deploy:
	@./scripts/deploy.sh $(REGISTRY)
