<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

Datetime rules:
    No .utcnow(), .utcfromtimestamp(), naive UTC
    Use: from datetime import datetime, timezone
    UTC now → datetime.now(timezone.utc)
    Fix deprecations on sight unless told otherwise
