"""
Invoke tasks for building HGL documentation.

Usage:
    inv install    - Install dependencies
    inv build      - Build documentation
    inv clean      - Clean build artifacts
    inv rebuild    - Clean and build
    inv serve      - Serve documentation locally
    inv watch      - Build and watch for changes
"""

from invoke import task
import os
import shutil
from pathlib import Path


# Project paths
DOCS_SOURCE = "docs/source"
BUILD_DIR = "build"
DOCS_DIR = "docs"


@task
def install(c):
    """Install project dependencies."""
    print("Installing dependencies...")
    c.run("pip install -r requirements.txt")
    print("Dependencies installed successfully!")


@task
def clean(c):
    """Clean build artifacts."""
    print("Cleaning build directory...")
    if os.path.exists(BUILD_DIR):
        shutil.rmtree(BUILD_DIR)
        print(f"Removed {BUILD_DIR}/")
    else:
        print(f"No {BUILD_DIR}/ directory to clean")

    # Also clean Sphinx cache
    doctrees = os.path.join(DOCS_DIR, ".doctrees")
    if os.path.exists(doctrees):
        shutil.rmtree(doctrees)
        print(f"Removed {doctrees}/")


@task
def build(c):
    """Build the Sphinx documentation."""
    print("Building documentation...")
    c.run(f"sphinx-build -b html {DOCS_SOURCE} {BUILD_DIR}", pty=True)
    print(f"\nDocumentation built successfully!")
    print(f"Open {BUILD_DIR}/index.html in your browser to view.")


@task(pre=[clean])
def rebuild(c):
    """Clean and rebuild documentation."""
    build(c)


@task
def serve(c, port=8000):
    """Serve the built documentation locally."""
    if not os.path.exists(BUILD_DIR):
        print("Build directory not found. Building documentation first...")
        build(c)

    print(f"\nServing documentation at http://localhost:{port}")
    print("Press Ctrl+C to stop the server")
    c.run(f"python -m http.server {port} -d {BUILD_DIR}", pty=True)


@task
def watch(c, port=8000):
    """Build documentation and watch for changes (requires sphinx-autobuild)."""
    print("Starting live documentation server...")
    print(f"Documentation will be available at http://localhost:{port}")
    print("Press Ctrl+C to stop the server")

    try:
        c.run(
            f"sphinx-autobuild {DOCS_SOURCE} {BUILD_DIR} --port {port} --open-browser",
            pty=True
        )
    except Exception as e:
        print("\nNote: sphinx-autobuild not installed.")
        print("Install it with: pip install sphinx-autobuild")
        print("Falling back to regular build + serve...")
        build(c)
        serve(c, port)


@task
def check(c):
    """Check documentation for issues (linkcheck, spelling, etc.)."""
    print("Checking documentation links...")
    c.run(f"sphinx-build -b linkcheck {DOCS_SOURCE} {BUILD_DIR}/linkcheck", pty=True)
    print("\nLink check complete. Results in build/linkcheck/")


@task
def publish(c):
    """Build documentation for production deployment."""
    print("Building documentation for production...")
    clean(c)
    c.run(f"sphinx-build -W -b html {DOCS_SOURCE} {BUILD_DIR}", pty=True)
    print("\nProduction build complete!")
    print("Documentation ready for deployment in build/")
