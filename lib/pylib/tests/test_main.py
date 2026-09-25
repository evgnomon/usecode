# License-Identifier: HGL
# Copyright (C) The Usecode Authors (see AUTHORS)

from typer.testing import CliRunner
from usecode import app

runner = CliRunner()


def test_hello():
    a = runner.invoke(app, ["hello"])
    assert type(a.exception) is SystemExit
