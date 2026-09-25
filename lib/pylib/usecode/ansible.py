# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

from dataclasses import dataclass, asdict
from typing import Optional


@dataclass
class AnsibleResult:
    changed: bool = False
    failed: bool = False
    skipped: bool = False
    unreachable: bool = False
    msg: Optional[str] = ""
    stderr: Optional[str] = None
    stdout: Optional[str] = None

    def to_dict(self):
        return asdict(self)
