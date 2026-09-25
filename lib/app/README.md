<!--
License-Identifier: HGL
Copyright (C) The Usecode Authors (see AUTHORS)
-->

# Frontend assets (HTMX + SCSS)

The UI is now rendered server-side by `lib/api` (Rust, Jinja2 templates) and enhanced with HTMX.

This directory only keeps SCSS source files used to generate API-served CSS.

## Build styles

```sh
cd lib/app
npm install
npm run build:css
```

Output: `../api/static/css/app.css`
