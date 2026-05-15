"""Blueprint CLI - Command-line interface for usecode.kit."""

import click

from usecode.kit.commands.vault import vault


@click.group()
def bp():
    """Blueprint development kit CLI."""
    pass


bp.add_command(vault)


if __name__ == "__main__":
    bp()
