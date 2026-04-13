"""Invoke tasks for bitframe project."""

import shutil
from pathlib import Path

from invoke import task


@task
def install(c):
    """Install the virtual environment and install the pre-commit hooks."""
    print("Creating virtual environment using uv")
    c.run("uv sync")
    c.run("uv run pre-commit install")


@task
def check(c):
    """Run code quality tools."""
    print("Checking lock file consistency with pyproject.toml")
    c.run("uv lock --locked")
    print("Linting code: Running pre-commit")
    c.run("uv run pre-commit run -a")
    print("Static type checking: Running mypy")
    c.run("uv run mypy")
    print("Checking for obsolete dependencies: Running deptry")
    c.run("uv run deptry src")


@task
def test(c):
    """Test the code with pytest."""
    c.run("uv run python -m pytest --cov --cov-config=pyproject.toml --cov-report=xml --cov-report=html --color=yes")


@task
def clean_build(c):
    """Clean build artifacts."""
    print("Removing build artifacts")
    dist_path = Path("dist")
    shutil.rmtree(dist_path, ignore_errors=True)


@task(pre=[clean_build])
def build(c):
    """Build wheel file."""
    print("Creating wheel file")
    c.run("uvx --from build pyproject-build --installer uv")


@task
def publish(c):
    """Publish a release to PyPI."""
    print("Publishing.")
    c.run("uvx twine upload --repository-url https://upload.pypi.org/legacy/ dist/*")


@task(pre=[build])
def build_and_publish(c):
    """Build and publish."""
    publish(c)


@task
def docs_test(c):
    """Test if documentation can be built without warnings or errors."""
    c.run("uv run mkdocs build -s")


@task
def docs(c):
    """Build and serve the documentation."""
    c.run("uv run mkdocs serve")
