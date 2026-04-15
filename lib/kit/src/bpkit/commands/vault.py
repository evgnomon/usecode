"""Vault commands for secret management."""

import click

from bpkit.vault import generate_password


@click.group()
def vault():
    """Vault operations for secret management."""
    pass


@vault.group()
def gen():
    """Generate cryptographic artifacts."""
    pass


@gen.command(name="pass")
@click.option(
    "--length",
    "-l",
    default=32,
    type=int,
    help="Length of the password (default: 32)",
)
@click.option(
    "--no-letters",
    is_flag=True,
    help="Exclude letters from password",
)
@click.option(
    "--no-digits",
    is_flag=True,
    help="Exclude digits from password",
)
@click.option(
    "--no-symbols",
    is_flag=True,
    help="Exclude symbols from password",
)
def generate_pass(length: int, no_letters: bool, no_digits: bool, no_symbols: bool):
    """Generate a cryptographically secure random password."""
    try:
        password = generate_password(
            length=length,
            use_letters=not no_letters,
            use_digits=not no_digits,
            use_symbols=not no_symbols,
        )
        click.echo(password)
    except ValueError as e:
        click.echo(f"Error: {e}", err=True)
        raise click.Abort from e
