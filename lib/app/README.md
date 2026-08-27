# Frontend assets (HTMX + SCSS)

The UI is now rendered server-side by FastAPI + Jinja2 in `lib/api` and enhanced with HTMX.

This directory only keeps SCSS source files used to generate API-served CSS.

## Build styles

```sh
cd lib/app
npm install
npm run build:css
```

Output: `../api/src/usecode_agent_api/static/css/app.css`
