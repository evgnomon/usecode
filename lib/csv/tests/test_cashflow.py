from decimal import Decimal
from pathlib import Path
from tempfile import NamedTemporaryFile

from bitframe import read_csv


def test_read_csv_basic():
    """Test basic CSV reading functionality."""
    csv_content = """Name,Amount
Item1,100.50
Item2,200.75
"""
    with NamedTemporaryFile(mode="w", suffix=".csv", delete=False) as f:
        f.write(csv_content)
        f.flush()
        df = read_csv(f.name, decimal_cols=["Amount"])

    assert len(df) == 2
    assert df["Name"].iloc[0] == "Item1"
    assert df["Amount"].iloc[0] == Decimal("100.50")
    assert df["Amount"].iloc[1] == Decimal("200.75")
    Path(f.name).unlink()


def test_read_csv_with_spaces():
    """Test CSV reading with spaces in values."""
    csv_content = """Name,  Amount
  Item1  ,  100.50
  Item2  ,  200.75
"""
    with NamedTemporaryFile(mode="w", suffix=".csv", delete=False) as f:
        f.write(csv_content)
        f.flush()
        df = read_csv(f.name, decimal_cols=["Amount"])

    assert df["Name"].iloc[0] == "Item1"
    assert df["Amount"].iloc[0] == Decimal("100.50")
    Path(f.name).unlink()


def test_read_csv_with_thousands_separator():
    """Test CSV reading with thousands separator."""
    csv_content = """Name,Amount
Item1,1_000.50
Item2,10_000.75
"""
    with NamedTemporaryFile(mode="w", suffix=".csv", delete=False) as f:
        f.write(csv_content)
        f.flush()
        df = read_csv(f.name, decimal_cols=["Amount"])

    assert df["Amount"].iloc[0] == Decimal("1000.50")
    assert df["Amount"].iloc[1] == Decimal("10000.75")
    Path(f.name).unlink()


def test_read_csv_with_comments():
    """Test CSV reading ignores comments."""
    csv_content = """Name,Amount
# This is a comment
Item1,100.50
Item2,200.75
"""
    with NamedTemporaryFile(mode="w", suffix=".csv", delete=False) as f:
        f.write(csv_content)
        f.flush()
        df = read_csv(f.name, decimal_cols=["Amount"])

    assert len(df) == 2
    Path(f.name).unlink()
