EXT_UUID = fzurls-focus@org.evgnomon
EXT_DIR = $(HOME)/.local/share/gnome-shell/extensions/$(EXT_UUID)
EXT_SRC_DIR = gnome-ext
EXT_FILES = extension.js metadata.json
INSTALLED_EXT_FILES = $(addprefix $(EXT_DIR)/,$(EXT_FILES))

.PHONY: install uninstall

install: $(INSTALLED_EXT_FILES)
	gsettings set org.gnome.shell disable-user-extensions false
	gnome-extensions enable $(EXT_UUID) 2>/dev/null || echo "Restart GNOME Shell, then run: gnome-extensions enable $(EXT_UUID)"

$(EXT_DIR)/%: $(EXT_SRC_DIR)/%
	install -d $(@D)
	install -m 0644 $< $@

uninstall:
	gnome-extensions disable $(EXT_UUID) 2>/dev/null || true
	rm -f $(INSTALLED_EXT_FILES)
	rmdir $(EXT_DIR) 2>/dev/null || true
