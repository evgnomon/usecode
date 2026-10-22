# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

.PHONY: all build install uninstall clean update

ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
BIN_DIR := $(BUILD_DIR)/usr/local/bin
PREFIX ?= /usr/local
INSTALL_DIR := $(PREFIX)/bin

GO_BINS := gron golangci-lint yj doctl godoc dlv
CARGO_BINS := drill
CARGO_ROOT := $(BIN_DIR:/bin=)
BIN_NAMES := $(GO_BINS) $(CARGO_BINS)
BUILD_BINS := $(addprefix $(BIN_DIR)/,$(BIN_NAMES))
INSTALL_BINS := $(addprefix $(INSTALL_DIR)/,$(BIN_NAMES))

all: build

build: $(BUILD_BINS)

install: $(INSTALL_BINS)

$(INSTALL_DIR)/%: $(BIN_DIR)/%
	@install -d $(INSTALL_DIR)
	@install -m 0755 $(BIN_DIR)/$* $@

$(BIN_DIR)/gron:
	@mkdir -p $(BIN_DIR)
	GOBIN=$(BIN_DIR) CGO_ENABLED=1 go install github.com/tomnomnom/gron@latest

$(BIN_DIR)/golangci-lint:
	@mkdir -p $(BIN_DIR)
	GOBIN=$(BIN_DIR) CGO_ENABLED=1 go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest

$(BIN_DIR)/yj:
	@mkdir -p $(BIN_DIR)
	GOBIN=$(BIN_DIR) CGO_ENABLED=1 go install github.com/sclevine/yj@latest

$(BIN_DIR)/doctl:
	@mkdir -p $(BIN_DIR)
	GOBIN=$(BIN_DIR) CGO_ENABLED=1 go install github.com/digitalocean/doctl/cmd/doctl@latest

$(BIN_DIR)/godoc:
	@mkdir -p $(BIN_DIR)
	GOBIN=$(BIN_DIR) CGO_ENABLED=1 go install golang.org/x/tools/cmd/godoc@latest

$(BIN_DIR)/dlv:
	@mkdir -p $(BIN_DIR)
	GOBIN=$(BIN_DIR) CGO_ENABLED=1 go install github.com/go-delve/delve/cmd/dlv@latest

$(BIN_DIR)/drill:
	@mkdir -p $(BIN_DIR)
	cargo install --root $(CARGO_ROOT) drill

uninstall:
	rm -f $(INSTALL_BINS)

clean:
	rm -f $(BUILD_BINS)

update:
	@find $(BUILD_BINS) -mtime +7 -delete 2>/dev/null || true
