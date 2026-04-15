version := 24.04.4.0

.PHONY: all build dist

all: dist

build: noble-server-cloudimg-amd64.img

noble-server-cloudimg-amd64.img:
	@wget https://cloud-images.ubuntu.com/noble/current/noble-server-cloudimg-amd64.img -O noble-server-cloudimg-amd64.img

dist/ubuntu-noble_$(version)_amd64.deb: noble-server-cloudimg-amd64.img
	@mkdeb -v $(version)

dist: dist/ubuntu-noble_$(version)_amd64.deb

publish:
	@pubdeb
