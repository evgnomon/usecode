# UseCode.dev


## Building the Documentation

### Quick Start with Invoke

The easiest way to build and work with the documentation is using invoke commands:

```bash
# Install dependencies
inv install

# Build the documentation
inv build

# Serve documentation locally at http://localhost:8000
inv serve

# Watch for changes and auto-rebuild (live reload)
inv watch

# Clean build artifacts
inv clean

# Clean and rebuild
inv rebuild

# Check links in documentation
inv check

# Build for production deployment
inv publish
```

### Manual Build (Alternative)

If you prefer to build manually:

1. Install dependencies:
```bash
pip install -r requirements.txt
```

2. Build the docs:
```bash
sphinx-build -b html docs/source build
```

3. View the documentation:
Open `build/index.html` in your browser.

## Deployment

The documentation is automatically built and deployed to GitHub Pages when changes are pushed to the main branch.

## License

HGL - The Last Software License
