#!/usr/bin/env bash

set -euo pipefail

dry_run=0
assume_yes=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            dry_run=1
            ;;
        --yes)
            assume_yes=1
            ;;
        *)
            echo "Usage: $0 [--dry-run] [--yes]" >&2
            exit 1
            ;;
    esac
    shift
done

run() {
    if [[ $dry_run -eq 1 ]]; then
        printf '+'
        printf ' %q' "$@"
        printf '\n'
    else
        "$@"
    fi
}

echo "Claude Code clean reinstall helper"
echo "Preferred path: stipe host doctor claude-code, then stipe host setup claude-code"
echo "This helper is destructive. It removes the global Claude CLI install and local cache data."

if [[ $dry_run -ne 1 && $assume_yes -ne 1 ]]; then
    read -r -p "Continue with reinstall? [y/N] " reply
    case "$reply" in
        [Yy]|[Yy][Ee][Ss]) ;;
        *)
            echo "Aborted."
            exit 1
            ;;
    esac
fi

timestamp="$(date +%Y%m%d-%H%M%S)"
npm_prefix="$(npm config get prefix)"

echo
echo "[1/5] Uninstalling Claude Code..."
run npm uninstall -g @anthropic-ai/claude-code

echo
echo "[2/5] Removing global npm artifacts..."
run rm -f "${npm_prefix}/bin/claude"
run rm -rf "${npm_prefix}/lib/node_modules/@anthropic-ai/claude-code"

echo
echo "[3/5] Removing Claude cache and local data..."
run rm -rf "${HOME}/.claude/downloads"
run rm -rf "${HOME}/.claude/local"

echo
echo "[4/5] Backing up legacy config if present..."
if [[ -f "${HOME}/.claude.json" ]]; then
    run cp "${HOME}/.claude.json" "${HOME}/.claude.json.backup-${timestamp}"
else
    echo "No legacy ~/.claude.json found"
fi

echo
echo "[5/5] Reinstalling Claude Code..."
run npm install -g @anthropic-ai/claude-code

echo
echo "Clean reinstall complete."
echo "Run 'claude --version' and 'stipe host doctor claude-code' to verify the install."
