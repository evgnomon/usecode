# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

IMAGE ?= localhost/usecode-base
# The cluster's registry, else the one its `registry` Ingress answers on.
REGISTRY ?=
TAG ?= $(shell git rev-parse --short HEAD)

.PHONY: image push

image:
	podman build --network=host -t $(IMAGE):$(TAG) -f Containerfile .

push: image
	./scripts/push.sh $(IMAGE) $(TAG) $(REGISTRY)
