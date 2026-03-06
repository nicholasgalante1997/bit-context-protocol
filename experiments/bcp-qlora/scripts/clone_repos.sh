#!/usr/bin/env bash
# scripts/clone_repos.sh
#
# Clones source repositories for training data generation.
# Uses shallow clones (--depth 1) to minimize disk usage.
#
# Usage:
#   bash scripts/clone_repos.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
REPOS_DIR="$PROJECT_DIR/data/repos"

mkdir -p "$REPOS_DIR"

clone_repo() {
    local url="$1"
    local name
    name="$(basename "$url" .git)"
    local dest="$REPOS_DIR/$name"

    if [ -d "$dest" ]; then
        echo "  [skip] $name (already exists)"
        return
    fi

    echo "  [clone] $name ← $url"
    git clone --depth 1 --quiet "$url" "$dest"
}

echo "=== Cloning source repositories ==="

# Rust projects
clone_repo "https://github.com/BurntSushi/ripgrep"
clone_repo "https://github.com/sharkdp/hyperfine"
clone_repo "https://github.com/Schniz/fnm"

# TypeScript projects
clone_repo "https://github.com/colinhacks/zod"
clone_repo "https://github.com/facebook/react"
clone_repo "https://github.com/nodejs/undici"

# Python projects
clone_repo "https://github.com/tiangolo/fastapi"
clone_repo "https://github.com/pydantic/pydantic"

# Go projects
clone_repo "https://github.com/junegunn/fzf"
clone_repo "https://github.com/containerd/nerdctl"
clone_repo "https://github.com/charmbracelet/bubbletea"

# BCP's own codebase
clone_repo "https://github.com/nicholasgalante1997/bit-context-protocol"

echo ""
echo "=== Done. Repos cloned to $REPOS_DIR ==="
ls -1 "$REPOS_DIR"
