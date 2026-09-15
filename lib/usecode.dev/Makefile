PORT ?= 8080

.PHONY: all sync serve check clean

all: check

sync:
	uv sync

serve: sync
	uv run uvicorn usecode_dev_site.app:app --host 0.0.0.0 --port $(PORT) --reload

check: sync
	uv run python -c "from usecode_dev_site.app import app; print(len(app.routes), 'routes')"

clean:
	rm -rf .venv dist
