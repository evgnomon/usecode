# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

NAME := fzurls
BIN := uc-pick-url
# The old name, kept for a release so scripts calling it carry on working.
ALIASES := fzurls
EXT_UUID := fzurls-focus@org.evgnomon
EXT_DIR := $(HOME)/.local/share/gnome-shell/extensions/$(EXT_UUID)
EXT_SRC_DIR := gnome-ext
EXT_FILES := extension.js metadata.json
INSTALLED_EXT_FILES := $(addprefix $(EXT_DIR)/,$(EXT_FILES))

include ../rust.mk

.PHONY: enable-ext remove-ext

link: enable-ext

enable-ext: $(INSTALLED_EXT_FILES)
	gsettings set org.gnome.shell disable-user-extensions false 2>/dev/null || true
	gnome-extensions enable $(EXT_UUID) 2>/dev/null || echo "Restart GNOME Shell, then run: gnome-extensions enable $(EXT_UUID)"

$(EXT_DIR)/%: $(EXT_SRC_DIR)/%
	install -d $(@D)
	install -m 0644 $< $@

uninstall: unlink remove-ext

remove-ext:
	gnome-extensions disable $(EXT_UUID) 2>/dev/null || true
	rm -f $(INSTALLED_EXT_FILES)
	rmdir $(EXT_DIR) 2>/dev/null || true
