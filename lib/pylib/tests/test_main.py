from typer.testing import CliRunner
from usecode import app

runner = CliRunner()


def test_hello():
    a = runner.invoke(app, ["hello"])
    assert type(a.exception) is SystemExit
