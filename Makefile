.PHONY: ci deploy publish version

$(eval $(shell ./scripts/ci_wrapper.sh --env 2>/dev/null))

VERSION := $(shell git describe --tags --match 'v*' --always 2>/dev/null | sed 's/^v//; s/-\([0-9]*\)-g/.dev\1+/')
export VERSION

LIB_DIRS := $(wildcard lib/*)


ci:
	@echo "══════════════════════════════════════════"
	@echo "  CI Environment"
	@echo "══════════════════════════════════════════"
	@echo "  CI_SYSTEM:       $(CI_SYSTEM)"
	@echo "  CI_EVENT:        $(CI_EVENT)"
	@echo "  CI_EVENT_PATH:   $(CI_EVENT_PATH)"
	@echo "  CI_STATE:        $(CI_STATE)"
	@echo "  CI_REF:          $(CI_REF)"
	@echo "  CI_BASE_REF:     $(CI_BASE_REF)"
	@echo "  CI_HEAD_REF:     $(CI_HEAD_REF)"
	@echo "  CI_COMMIT:       $(CI_COMMIT)"
	@echo "  CI_COMMIT_SHORT: $(CI_COMMIT_SHORT)"
	@echo "  CI_MSG:          $(CI_MSG)"
	@echo "  CI_BRANCH:       $(CI_BRANCH)"
	@echo "  CI_TARGET:       $(CI_TARGET)"
	@echo "  CI_ENV:          $(CI_ENV)"
	@echo "  CI_TRACK:        $(CI_TRACK)"
	@echo "  CI_TAG:          $(CI_TAG)"
	@echo "  CI_OWNER:        $(CI_OWNER)"
	@echo "  CI_REPO:         $(CI_REPO)"
	@echo "  CI_SLUG:         $(CI_SLUG)"
	@echo "  CI_URL:          $(CI_URL)"
	@echo "  CI_CHANGE:       $(CI_CHANGE)"
	@echo "  CI_RUN:          $(CI_RUN)"
	@echo "  CI_RUN_URL:      $(CI_RUN_URL)"
	@echo "  CI_ACTOR:        $(CI_ACTOR)"
	@echo "  CI_EMAIL:        $(CI_EMAIL)"
	@echo "  CI_PIPELINE:     $(CI_PIPELINE)"
	@echo "  CI_JOB:          $(CI_JOB)"
	@echo "  CI_TIMESTAMP:    $(CI_TIMESTAMP)"
	@echo "  CI_WORKSPACE:    $(CI_WORKSPACE)"
	@echo "  CI_DIR:          $(CI_DIR)"
	@echo "  CI_PARENT:       $(CI_PARENT)"
	@echo "══════════════════════════════════════════"
 
deploy:
	@echo "Deploying $(CI_COMMIT_SHORT) from $(CI_BRANCH) [env=$(CI_ENV) track=$(CI_TRACK)]..."

version:
	@echo $(VERSION)

publish:
	@for d in $(LIB_DIRS); do \
		if $(MAKE) -C $$d -n publish >/dev/null 2>&1; then \
			$(MAKE) -C $$d publish || exit $$?; \
		else \
			echo "Skipping $$d (no publish target)"; \
		fi; \
	done

