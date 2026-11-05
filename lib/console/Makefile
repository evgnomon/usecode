# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

IMAGE ?= localhost/usecode-console
# The cluster's registry, else the one its `registry` Ingress answers on.
# CONTAINER_REGISTRY: where `make push` sends the image. Falls back to the
# registry Ingress's host, or the legacy REGISTRY variable.
REGISTRY ?= $(CONTAINER_REGISTRY)
# The latest push: the commit origin/master is at.
TAG ?= $(shell ./scripts/k8s.sh tag)
PORT ?= 8080
# The API the pages call; `make -C lib/api run` starts one here.
API_URL ?= http://localhost:8000

.PHONY: all build test serve dev image push deploy rollout clean

all: build

node_modules: package.json package-lock.json
	npm ci --silent --no-audit --no-fund
	@touch $@

build: node_modules
	npm run --silent build

test: node_modules
	npm test

serve: build
	API_URL=$(API_URL) PORT=$(PORT) PUBLIC_URL=http://localhost:$(PORT) node dist/server/main.js

# The pages with hot reload on :5173; they call the API at $(API_URL).
dev: node_modules
	npx vite

# Builds the latest push, not the working tree.
image:
	./scripts/k8s.sh image $(IMAGE)

push: image
	./scripts/push.sh $(IMAGE) $(TAG) $(REGISTRY)

# Builds, pushes and rolls out the latest push, then checks the site answers.
deploy:
	./scripts/deploy.sh $(REGISTRY)

# Only rolls the console out at the latest push; `make push` it first.
rollout:
	./scripts/k8s.sh deploy $(REGISTRY)

clean:
	rm -rf dist node_modules
