"""Blueprint CLI - Command-line interface for bpkit."""

import click

from bpkit.commands.vault import vault


@click.group()
def bp():
    """Blueprint development kit CLI."""
    pass


bp.add_command(vault)


if __name__ == "__main__":
    bp()
