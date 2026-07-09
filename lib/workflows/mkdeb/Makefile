INSTALL_DIR := /usr/local/bin
BINS := $(notdir $(wildcard bin/*))

.PHONY: install uninstall

install:
	install -d $(INSTALL_DIR)
	$(foreach b,$(BINS),install -m 755 src/$(b) $(INSTALL_DIR)/$(b);)

uninstall:
	$(foreach b,$(BINS),rm -f $(INSTALL_DIR)/$(b);)
