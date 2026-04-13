# bitframe

A Python library for reading and processing CSV files with decimal precision.

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

- Read CSV files with proper decimal handling
- Automatic whitespace trimming
- Support for thousands separators (underscore)
- Comment support (lines starting with #)
- Empty line handling
