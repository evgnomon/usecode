.PHONY: bootstrap check test clean-build clean install uninstall publish build-and-publish docs docs-test help

ROOT_DIR ?= $(abspath $(CURDIR)/../..)
BUILD_DIR ?= $(ROOT_DIR)/build
DESTDIR ?= /
STAMP_DIR := $(ROOT_DIR)/.stamps/$(patsubst $(ROOT_DIR)/%,%,$(CURDIR))
COMPONENT_BUILD_DIR := $(CURDIR)/build
COMPONENT_STAGE_ROOT := $(COMPONENT_BUILD_DIR)/stage
COMPONENT_STAGE_PREFIX := $(COMPONENT_STAGE_ROOT)/usr/local
FILES_MANIFEST := $(STAMP_DIR)/installed-files.txt
DIRS_MANIFEST := $(STAMP_DIR)/installed-dirs.txt
BUILD_STAMP := $(STAMP_DIR)/build.stamp

NAME := $(shell awk -F'"' '/^name = / {print $$2}' pyproject.toml)
VERSION := $(shell awk -F'"' '/^version = / {print $$2}' pyproject.toml)
WHEEL := dist/$(NAME)-$(VERSION)-py3-none-any.whl
SDIST := dist/$(NAME)-$(VERSION).tar.gz
SOURCES := $(shell find src -type f) pyproject.toml README.md

bootstrap: ## Install the virtual environment and pre-commit hooks
	@echo "Creating virtual environment using uv"
	@uv sync
	@uv run pre-commit install

check: ## Run code quality tools
	@echo "Checking lock file consistency with pyproject.toml"
	@uv lock --locked
	@echo "Linting code: Running pre-commit"
	@uv run pre-commit run -a
	@echo "Static type checking: Running mypy"
	@uv run mypy
	@echo "Checking for obsolete dependencies: Running deptry"
	@uv run deptry src

test: ## Test the code with pytest
	@uv run python -m pytest --cov --cov-config=pyproject.toml --cov-report=xml --cov-report=html --color=yes

clean-build: ## Clean build artifacts
	@echo "Removing build artifacts"
	@rm -rf dist

clean: clean-build ## Remove build artifacts and staged tree
	@if [ -f $(FILES_MANIFEST) ]; then while IFS= read -r f; do rm -f "$(BUILD_DIR)/$$f"; done < $(FILES_MANIFEST); fi
	@if [ -f $(DIRS_MANIFEST) ]; then while IFS= read -r d; do rmdir "$(BUILD_DIR)/$$d" 2>/dev/null || true; done < $(DIRS_MANIFEST); fi
	@rm -rf $(COMPONENT_BUILD_DIR)
	@rm -rf $(STAMP_DIR)
	@rmdir $(ROOT_DIR)/.stamps 2>/dev/null || true

build: $(BUILD_STAMP) ## Build wheel and stage package + deps into the shared build tree

$(BUILD_STAMP): $(WHEEL)
	@echo "Staging $(NAME) + deps into $(COMPONENT_STAGE_PREFIX) and linking into $(BUILD_DIR)/usr/local"
	@mkdir -p $(STAMP_DIR)
	@if [ -f $(FILES_MANIFEST) ]; then while IFS= read -r f; do rm -f "$(BUILD_DIR)/$$f"; done < $(FILES_MANIFEST); fi
	@if [ -f $(DIRS_MANIFEST) ]; then while IFS= read -r d; do rmdir "$(BUILD_DIR)/$$d" 2>/dev/null || true; done < $(DIRS_MANIFEST); fi
	@rm -rf $(COMPONENT_STAGE_ROOT)
	@mkdir -p $(COMPONENT_STAGE_PREFIX)
	@uv run --with pip python -m pip install \
		--prefix=$(COMPONENT_STAGE_PREFIX) --no-compile --no-warn-script-location \
		$(WHEEL)
	@cd $(COMPONENT_STAGE_ROOT) && find usr/local \( -type f -o -type l \) -print > $(FILES_MANIFEST)
	@cd $(COMPONENT_STAGE_ROOT) && find usr/local -depth -type d -print > $(DIRS_MANIFEST)
	@while IFS= read -r d; do mkdir -p "$(BUILD_DIR)/$$d"; done < $(DIRS_MANIFEST)
	@while IFS= read -r f; do \
		mkdir -p "$(BUILD_DIR)/$$(dirname "$$f")"; \
		ln -sfn "$(COMPONENT_STAGE_ROOT)/$$f" "$(BUILD_DIR)/$$f"; \
	done < $(FILES_MANIFEST)
	@touch $@

$(WHEEL) $(SDIST) &: $(SOURCES)
	@echo "Creating wheel file"
	@rm -rf dist
	@uvx --from build pyproject-build --installer uv

install: $(BUILD_STAMP) ## Copy staged tree into $(DESTDIR)
	@test -f $(FILES_MANIFEST) || { echo "No install manifest at $(FILES_MANIFEST); run 'make build' first" >&2; exit 1; }
	@while IFS= read -r f; do \
		dst='$(DESTDIR)'"$$f"; \
		mkdir -p "$$(dirname "$$dst")"; \
		cp -L --remove-destination --preserve=mode,timestamps "$(BUILD_DIR)/$$f" "$$dst"; \
	done < $(FILES_MANIFEST)

uninstall: ## Remove files from $(DESTDIR) matching the staged tree
	@echo "Uninstalling from $(DESTDIR)"
	@test -f $(FILES_MANIFEST) || { echo "No install manifest at $(FILES_MANIFEST); run 'make build' first" >&2; exit 1; }
	@while IFS= read -r f; do rm -f '$(DESTDIR)'"$$f"; done < $(FILES_MANIFEST)
	@while IFS= read -r d; do rmdir '$(DESTDIR)'"$$d" 2>/dev/null || true; done < $(DIRS_MANIFEST)

publish: ## Publish a release to PyPI
	@echo "Publishing."
	@uvx twine upload --repository-url https://upload.pypi.org/legacy/ dist/*

build-and-publish: build publish ## Build and publish

docs-test: ## Test if documentation can be built without warnings or errors
	@uv run mkdocs build -s

docs: ## Build and serve the documentation
	@uv run mkdocs serve

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-20s\033[0m %s\n", $$1, $$2}'

.DEFAULT_GOAL := build
