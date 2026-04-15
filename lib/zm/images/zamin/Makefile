prefix := /usr
version := 0.1.0

all: dist

.PHONY: build build-image clean install dist publish

build: build-image

dist: dist/zamin_$(version)_amd64.deb

dist/zamin_$(version)_amd64.deb: output-zamin/zamin
	@mkdeb --version $(version)

publish: dist
	@pubdeb

build-image: output-zamin/zamin

install: output-zamin/zamin
	@mkdir -p $(prefix)/share/zm/images
	@cp output-zamin/zamin $(prefix)/share/zm/images/zamin

uninstall:
	@rm -f $(prefix)/share/zm/images/zamin

clean:
	@rm -rf output-zamin

output-zamin/zamin: zamin.pkr.hcl
	@rm -rf output-zamin
	@packer build zamin.pkr.hcl
