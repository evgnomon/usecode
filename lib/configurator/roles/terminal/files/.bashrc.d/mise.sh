# Activate mise (https://mise.jdx.dev) shims and per-directory environments
if command -v mise >/dev/null 2>&1; then
  eval "$(mise activate bash)"
fi
