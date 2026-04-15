"""
GPG-based secret encryption/decryption utility.
Processes data in memory without temporary files.
"""

import json
import os
import secrets as secretslib
import shutil
import string
import subprocess
import sys
from pathlib import Path

from bpkit.config import blueprint_config

SECRETS_DIR = Path.home() / ".config" / "blueprint" / "secrets"


class GPGNotFoundError(FileNotFoundError):
    """Raised when gpg command is not found in system PATH."""

    def __init__(self) -> None:
        super().__init__("gpg command not found")


class GPGKeyNotConfiguredError(ValueError):
    """Raised when GPG key is not configured."""

    def __init__(self) -> None:
        super().__init__("GPG key not configured")


def get_gpg_path() -> str:
    """Get the full path to the gpg executable."""
    gpg_path = shutil.which("gpg")
    if not gpg_path:
        raise GPGNotFoundError
    return gpg_path


def user_gpg_key() -> str:
    """
    Retrieve the user's GPG key identifier from environment variable or use default.

    Returns:
        GPG key identifier as a string.
    """
    user_key = os.getenv("BP_GPG_KEY", blueprint_config.gpg.key)
    if not user_key:
        raise GPGKeyNotConfiguredError
    return user_key


def generate_password(
    length: int = 32,
    use_letters: bool = True,
    use_digits: bool = True,
    use_symbols: bool = True,
) -> str:
    """
    Generate a cryptographically secure random password.

    Args:
        length: Length of the password (default: 20)
        use_letters: Include uppercase and lowercase letters (default: True)
        use_digits: Include digits 0-9 (default: True)
        use_symbols: Include special characters (default: True)

    Returns:
        A randomly generated password string.

    Raises:
        ValueError: If length is less than 1 or no character sets are selected.
    """
    if length < 1:
        msg = "Password length must be at least 1"
        raise ValueError(msg)

    # Build character set
    chars = ""
    if use_letters:
        chars += string.ascii_letters
    if use_digits:
        chars += string.digits
    if use_symbols:
        chars += string.punctuation

    if not chars:
        msg = "At least one character set must be selected"
        raise ValueError(msg)

    # Generate password using cryptographically secure random
    password = "".join(secretslib.choice(chars) for _ in range(length))
    return password


def encrypt_file(filename: str, gpg_key: str) -> None:
    """
    Encrypt data from stdin using GPG and save to secrets directory.

    Args:
        filename: Name for the encrypted file (will add .asc extension)
        gpg_key: GPG key identifier for recipient
    """
    gpg_key = gpg_key or user_gpg_key()
    if not filename:
        print("Please provide a file to encrypt and the recipient's key identifier.")
        sys.exit(1)

    # Create secrets directory if it doesn't exist
    SECRETS_DIR.mkdir(parents=True, exist_ok=True)

    # Read from stdin
    input_data = sys.stdin.buffer.read()

    # Encrypt using GPG (process in memory)
    try:
        gpg_path = get_gpg_path()
        # Safe: gpg_path from shutil.which, args are controlled, no shell execution
        result = subprocess.run(  # noqa: S603
            [gpg_path, "-e", "-r", gpg_key, "--armor"],
            input=input_data,
            capture_output=True,
            check=True,
        )

        # Write encrypted output to file
        output_path = SECRETS_DIR / f"{filename}.asc"
        output_path.write_bytes(result.stdout)
        print(f"Encrypted and saved to {output_path}")

    except subprocess.CalledProcessError as e:
        print(f"Encryption failed: {e.stderr.decode()}", file=sys.stderr)
        sys.exit(1)
    except FileNotFoundError:
        print("Error: gpg command not found. Please install GnuPG.", file=sys.stderr)
        sys.exit(1)


def decrypt_file(filename: str) -> None:
    """
    Decrypt a file from secrets directory and output to stdout.

    Args:
        filename: Name of the file to decrypt (without .asc extension)
    """
    if not filename:
        print("Please provide a file to decrypt.")
        sys.exit(1)

    # Remove any trailing .asc extension if provided
    filename = filename.rstrip(".asc")

    # Construct file path
    file_path = SECRETS_DIR / f"{filename}.asc"

    if not file_path.exists():
        print(f"Error: File not found: {file_path}", file=sys.stderr)
        sys.exit(1)

    # Read encrypted file
    encrypted_data = file_path.read_bytes()

    # Decrypt using GPG (process in memory)
    try:
        gpg_path = get_gpg_path()
        # Safe: gpg_path from shutil.which, args are controlled, no shell execution
        result = subprocess.run([gpg_path, "--quiet", "-d"], input=encrypted_data, capture_output=True, check=True)  # noqa: S603

        # Output decrypted data to stdout
        sys.stdout.buffer.write(result.stdout)

    except subprocess.CalledProcessError as e:
        print(f"Decryption failed: {e.stderr.decode()}", file=sys.stderr)
        sys.exit(1)
    except FileNotFoundError:
        print("Error: gpg command not found. Please install GnuPG.", file=sys.stderr)
        sys.exit(1)


def secrets(name=""):
    # Set environment variable
    os.environ["EDITOR"] = "vi"

    # Determine SECRET_FILE
    home = Path.home()
    if name:
        secret_file = home / ".blueprint" / "secrets" / f"{name}.yaml"
    else:
        secret_file = home / ".blueprint" / "secrets" / "secrets.yaml"

    # Determine VAULT_FILE
    vault_file = name + ".vault" if name else "vault"

    vault_file_path = home / ".blueprint" / "secrets" / f"{vault_file}.asc"

    # Check if vault file exists
    if not vault_file_path.exists():
        print(f"Vault file not found: {vault_file_path}")
        sys.exit(1)

    # Check if secret file exists
    if not secret_file.exists():
        print(f"Secret file not found: {secret_file}")
        sys.exit(1)

    # Get vault password by running: vault -d $VAULT_FILE
    try:
        vault_password = subprocess.check_output(  # noqa: S603
            ["vaultpy", "-d", vault_file.replace(".asc", "")],  # noqa: S607
            text=True,
            stderr=subprocess.PIPE,
        ).strip()
    except subprocess.CalledProcessError as e:
        print(f"Error getting vault password: {e}")
        sys.exit(1)

    # Run ansible-vault view with password piped to stdin
    ansible_vault = subprocess.Popen(  # noqa: S603
        [  # noqa: S607
            "ansible-vault",
            "view",
            str(secret_file),
            "--vault-password-file",
            "/dev/stdin",
        ],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    # Pipe output to yj
    yj = subprocess.Popen(
        ["yj"],  # noqa: S607
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    # Send password to ansible-vault and pipe its output to yj
    vault_stdout, vault_stderr = ansible_vault.communicate(input=vault_password)

    if ansible_vault.returncode != 0:
        print(f"Error from ansible-vault: {vault_stderr}", file=sys.stderr)
        sys.exit(1)

    # Send ansible-vault output to yj
    yj_output, yj_error = yj.communicate(input=vault_stdout)

    if yj.returncode != 0:
        print(f"Error from yj: {yj_error}", file=sys.stderr)
        sys.exit(1)

    return json.loads(yj_output)  # Validate JSON
