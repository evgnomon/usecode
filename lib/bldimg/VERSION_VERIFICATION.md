# Dockerfile Version Verification Report

**Date:** 2025-12-24  
**Triggered by:** hg@evgnomon.org  
**Workflow:** Maintain Dockerfile

## Verification Results

All packages in the Dockerfile are using their latest stable versions:

| Package | Current Version | Latest Version | Status |
|---------|----------------|----------------|--------|
| Debian Base | bookworm | bookworm (stable) | ✅ Up-to-date |
| Node.js | v24.12.0 | v24.12.0 (LTS) | ✅ Up-to-date |
| Go | 1.25.5 | 1.25.5 | ✅ Up-to-date |
| Fish Shell | 4.2.1 | 4.2.1 | ✅ Up-to-date |
| GitHub CLI | 2.83.2 | 2.83.2 | ✅ Up-to-date |
| Docker Compose | 5.0.1 | 5.0.1 | ✅ Up-to-date |
| GolangCI-Lint | latest | latest | ✅ Up-to-date |

## Verification Commands Used

- Node.js LTS: `curl -s https://nodejs.org/dist/index.json | jq -r '[.[] | select(.lts != false)] | .[0] | .version'`
- Go: `curl -s https://go.dev/dl/?mode=json | jq -r '.[0].version'`
- Fish Shell: `curl -s https://api.github.com/repos/fish-shell/fish-shell/releases/latest | jq -r '.tag_name'`
- GitHub CLI: `curl -s https://api.github.com/repos/cli/cli/releases/latest | jq -r '.tag_name'`
- Docker Compose: `curl -s https://api.github.com/repos/docker/compose/releases/latest | jq -r '.tag_name'`

## Conclusion

No updates required. The Dockerfile is already using the latest stable versions of all packages.
