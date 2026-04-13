from decimal import Decimal, InvalidOperation
from typing import Optional

import pandas as pd


def read_csv(
    p: str,
    decimal_places: int = 2,
    decimal_cols: Optional[list[str]] = None,
) -> pd.DataFrame:
    """Read a CSV file with special handling for decimal columns.

    Args:
        p: Path to the CSV file.
        decimal_places: Number of decimal places to use for decimal columns.
        decimal_cols: List of column names to convert to Decimal type.

    Returns:
        A pandas DataFrame with the CSV data.
    """
    if decimal_cols is None:
        decimal_cols = []
    df = pd.read_csv(
        p,
        skipinitialspace=True,  # removes spaces after every comma (the magic one)
        skip_blank_lines=True,  # ignores empty lines like "         ,                               ,"
        thousands="_",  # lets you write 50,000 instead of 50000 (optional)
        comment="#",  # ignores any line starting with # (for notes)
        engine="python",  # needed for skip_blank_lines + flexibility)
        sep=",",
    )
    df = df.dropna(how="all")
    df.columns = df.columns.str.strip()
    for a in df.columns:
        if df[a].dtype == "object":
            df[a] = df[a].str.strip()

    for col in decimal_cols:
        df[col] = (
            df[col]
            .astype(str)  # in case it's object
            .str.strip()  # remove leading/trailing spaces
            .str.replace("_", "")  # remove thousands separators
            .str.replace(r"[^\d.-]", "", regex=True)  # remove any junk
            .replace({"": None, "nan": None})  # empty → NaN
        )

    def to_decimal(val: object) -> Decimal:
        if pd.isna(val):
            return Decimal("0." + "0" * decimal_places)
        cleaned = str(val).strip().replace(",", "")
        try:
            return Decimal(cleaned).quantize(Decimal("0.00"))
        except (InvalidOperation, ValueError):
            return Decimal("0.00")

    for col in decimal_cols:
        df[col] = df[col].apply(to_decimal)

    return df
