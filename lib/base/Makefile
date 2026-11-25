# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

IMAGE ?= localhost/usecode-base
# The cluster's registry, else the one its `registry` Ingress answers on.
REGISTRY ?=
TAG ?= $(shell git rev-parse --short HEAD)
# The uc release mise installs in the image: the newest v* tag.
UC_VERSION ?= $(shell git describe --tags --match 'v*' --abbrev=0 2>/dev/null | sed 's/^v//')

.PHONY: image push

image:
	podman build --network=host --build-arg UC_VERSION=$(or $(UC_VERSION),latest) -t $(IMAGE):$(TAG) -f Containerfile .

push: image
	./scripts/push.sh $(IMAGE) $(TAG) $(REGISTRY)
