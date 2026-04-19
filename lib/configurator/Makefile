.PHONY: all build install install-eget uninstall clean

EGET_VERSION ?= latest
ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
BIN_DIR := $(BUILD_DIR)/usr/local/bin
PREFIX ?= /usr/local
INSTALL_DIR := $(PREFIX)/bin
EGET ?= $(BIN_DIR)/eget


EGET_BINS := eget gh fd delta rg bat hugo rclone hcloud tflint gvproxy goose kompose csvlens yq
GO_BINS := gron golangci-lint yj doctl godoc dlv
CARGO_BINS := drill
CARGO_ROOT := $(BIN_DIR:/bin=)
BIN_NAMES := $(EGET_BINS) $(GO_BINS) $(CARGO_BINS)
BUILD_BINS := $(addprefix $(BIN_DIR)/,$(BIN_NAMES))
INSTALL_BINS := $(addprefix $(INSTALL_DIR)/,$(BIN_NAMES))

all: build

build: $(BUILD_BINS)

install: $(INSTALL_BINS)

$(INSTALL_DIR)/%: $(BIN_DIR)/%
	@install -d $(INSTALL_DIR)
	@install -m 0755 $(BIN_DIR)/$* $@

install-eget: $(EGET)

$(EGET):
	@mkdir -p $(BIN_DIR)
	GOBIN=$(BIN_DIR) go install github.com/zyedidia/eget@$(EGET_VERSION)

$(BIN_DIR)/gh: | $(EGET)
	$(EGET) --to $(BIN_DIR) -a tar.gz cli/cli

$(BIN_DIR)/fd: | $(EGET)
	$(EGET) --to $(BIN_DIR) -a gnu sharkdp/fd

$(BIN_DIR)/delta: | $(EGET)
	$(EGET) --to $(BIN_DIR) -a gnu dandavison/delta

$(BIN_DIR)/rg: | $(EGET)
	$(EGET) --to $(BIN_DIR) BurntSushi/ripgrep

$(BIN_DIR)/bat: | $(EGET)
	$(EGET) --to $(BIN_DIR) -a gnu sharkdp/bat

$(BIN_DIR)/hugo: | $(EGET)
	$(EGET) --to $(BIN_DIR) -a extended -a tar.gz -a withdeploy gohugoio/hugo

$(BIN_DIR)/rclone: | $(EGET)
	$(EGET) --to $(BIN_DIR) -a zip rclone/rclone

$(BIN_DIR)/hcloud: | $(EGET)
	$(EGET) --to $(BIN_DIR) -f hcloud -a ^sig hetznercloud/cli

$(BIN_DIR)/tflint: | $(EGET)
	$(EGET) --to $(BIN_DIR)/tflint terraform-linters/tflint

$(BIN_DIR)/gvproxy: | $(EGET)
	$(EGET) --to $(BIN_DIR)/gvproxy containers/gvisor-tap-vsock

$(BIN_DIR)/goose: | $(EGET)
	$(EGET) --to $(BIN_DIR) -a unknown-linux-gnu.tar.bz2 aaif-goose/goose

$(BIN_DIR)/kompose: | $(EGET)
	$(EGET) --to $(BIN_DIR)/kompose -a tar.gz kubernetes/kompose

$(BIN_DIR)/csvlens: | $(EGET)
	$(EGET) --to $(BIN_DIR) -a gnu YS-L/csvlens

$(BIN_DIR)/yq: | $(EGET)
	$(EGET) --to $(BIN_DIR)/yq -a tar.gz -f yq_linux_amd64 mikefarah/yq

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
