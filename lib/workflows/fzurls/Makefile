EXT_UUID = fzurls-focus@org.evgnomon
EXT_DIR = $(HOME)/.local/share/gnome-shell/extensions/$(EXT_UUID)

install:
	mkdir -p $(EXT_DIR)
	cp gnome-ext/* $(EXT_DIR)/
	gsettings set org.gnome.shell disable-user-extensions false
	gnome-extensions enable $(EXT_UUID) 2>/dev/null || echo "Restart GNOME Shell, then run: gnome-extensions enable $(EXT_UUID)"

uninstall:
	gnome-extensions disable $(EXT_UUID) 2>/dev/null || true
	rm -rf $(EXT_DIR)
