# Getting started

```bash
poetry install
poetry run poe check
pip install -e . # for ansible to find the collection
```

# Release a new version

Version string is in `pyproject.toml` and `ansible_collections/evgnomon/catamaran/galaxy.yml` should be the same.
Upgrade with poetry then set the same version in `galaxy.yml`.

```
poetry version patch
# Set the same version in ansible_collections/evgnomon/catamaran/galaxy.yml
```

