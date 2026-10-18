# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

IMAGE ?= localhost/usecode-console
# The cluster's registry, else the one its `registry` Ingress answers on.
# CONTAINER_REGISTRY: where `make push` sends the image. Falls back to the
# registry Ingress's host, or the legacy REGISTRY variable.
REGISTRY ?= $(CONTAINER_REGISTRY)
TAG ?= $(shell git rev-parse --short HEAD)
PORT ?= 8080
TEST_DB_PORT ?= 55432
TEST_DATABASE_URL ?= postgres://postgres:test@127.0.0.1:$(TEST_DB_PORT)/console

.PHONY: all build test test-db test-db-stop serve dev image push clean

all: build

node_modules: package.json package-lock.json
	npm ci --silent --no-audit --no-fund
	@touch $@

build: node_modules
	npm run --silent build

# The signup and billing flow runs too while `make test-db` is up.
test: node_modules
	TEST_DATABASE_URL=$(TEST_DATABASE_URL) npm test

test-db:
	@./scripts/test-db.sh start $(TEST_DB_PORT)

test-db-stop:
	@./scripts/test-db.sh stop

serve: build
	DATABASE_URL=$(TEST_DATABASE_URL) PORT=$(PORT) PUBLIC_URL=http://localhost:$(PORT) node dist/server/main.js

# The pages with hot reload on :5173, talking to `make serve`.
dev: node_modules
	API_URL=http://localhost:$(PORT) npx vite

image:
	podman build --network=host -t $(IMAGE):$(TAG) -f Containerfile .

push: image
	./scripts/push.sh $(IMAGE) $(TAG) $(REGISTRY)

clean:
	rm -rf dist node_modules
