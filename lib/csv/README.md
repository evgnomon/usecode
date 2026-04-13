# bitframe

[![Release](https://img.shields.io/github/v/release/evgnomon/bitframe)](https://img.shields.io/github/v/release/evgnomon/bitframe)
[![Build status](https://img.shields.io/github/actions/workflow/status/evgnomon/bitframe/main.yml?branch=main)](https://github.com/evgnomon/bitframe/actions/workflows/main.yml?query=branch%3Amain)
[![License](https://img.shields.io/github/license/evgnomon/bitframe)](https://img.shields.io/github/license/evgnomon/bitframe)

A Python library for reading and processing CSV files with decimal precision.

- **Github repository**: <https://github.com/evgnomon/bitframe/>
- **Documentation** <https://evgnomon.github.io/bitframe/>

## Installation

```bash
pip install bitframe
```

Or with uv:

```bash
uv add bitframe
```

## Usage

```python
from bitframe import read_csv

# Read a CSV file with decimal columns
df = read_csv("data.csv", decimal_cols=["Amount", "Price"])
```

## Features

- Read CSV files with proper decimal handling using Python's `Decimal` type
- Automatic whitespace trimming for both headers and values
- Support for thousands separators (underscore: `1_000`)
- Comment support (lines starting with `#`)
- Empty line handling

## Development

This project uses [uv](https://github.com/astral-sh/uv) for dependency management.

```bash
# Install dependencies
uv sync

# Run tests
uv run invoke test

# Run linting and type checks
uv run invoke check

# Build documentation
uv run invoke docs
```

## License

MIT License - see [LICENSE](LICENSE) for details.
