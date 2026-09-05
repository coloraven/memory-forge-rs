#!/usr/bin/env bash
set -euo pipefail
cd /c/Users/Administrator/Documents/GitHub/memory-forge-rs
git add -A
git status -sb
git diff --cached --stat
git commit -m "$(cat <<'EOF'
feat: add ZCode sessions and Windows-only CI builds [build]

Default CI builds Windows artifacts; macOS/Linux stay available via
workflow_dispatch. ZCode reads ~/.zcode/cli/db/db.sqlite (OpenCode-like).

EOF
)"
git push origin HEAD
git status -sb
