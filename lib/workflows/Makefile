.PHONY: build install clean

PKGS:= bin fzurls mkdeb pg workd

build install clean:
	@for pkg in $(PKGS); do \
		[ -f $$pkg/Makefile ] && $(MAKE) -C $$pkg $@ || true; \
	done
