Datetime rules:
    No .utcnow(), .utcfromtimestamp(), naive UTC
    Use: from datetime import datetime, timezone
    UTC now → datetime.now(timezone.utc)
    Fix deprecations on sight unless told otherwise
