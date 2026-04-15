# Configuration file for the Sphinx documentation builder.

# -- Project information -----------------------------------------------------
project = 'HGL - The Last Software License'
copyright = '2025, UseCode'
author = 'UseCode'

# -- General configuration ---------------------------------------------------
extensions = [
    'myst_parser',
]

templates_path = ['_templates']
exclude_patterns = []

# -- Options for HTML output -------------------------------------------------
html_theme = 'furo'
html_static_path = ['_static']
html_title = "Use Code Dev"

# Furo theme options
html_theme_options = {
    "navigation_with_keys": True,
}

# MyST Parser configuration
myst_enable_extensions = [
    "colon_fence",
]
