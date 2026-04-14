REGISTRY ?= ghcr.io
OWNER ?= evgnomon
NAME ?= ark
TAG ?= latest
IMAGE ?= $(REGISTRY)/$(OWNER)/$(NAME):$(TAG)

STAMP_DIR := .stamps
BUILD_STAMP := $(STAMP_DIR)/build
PUBLISH_STAMP := $(STAMP_DIR)/publish
SOURCES := Dockerfile evgnomon.asc evgnomon.sources SHA512SUMS $(shell find playbooks -type f 2>/dev/null)

.PHONY: build publish clean

build: $(BUILD_STAMP)

$(BUILD_STAMP): $(SOURCES)
	@mkdir -p $(STAMP_DIR)
	podman build -t $(IMAGE) .
	@touch $@

publish: $(PUBLISH_STAMP)

$(PUBLISH_STAMP): $(BUILD_STAMP)
	@mkdir -p $$HOME/.cache/ark; \
	AUTH=$$(mktemp -u $$HOME/.cache/ark/auth.XXXXXX); trap "rm -f $$AUTH" EXIT; \
	getsecret | jq -r '.github_pat' | podman login --authfile $$AUTH $(REGISTRY) -u $(OWNER) --password-stdin && \
	podman push --authfile $$AUTH $(IMAGE)
	@touch $@

clean:
	rm -rf $(STAMP_DIR)
