#!/usr/bin/env bash

set -euo pipefail

echo
echo "=== Claude Code Health Check ==="
echo "Preferred path: stipe doctor && stipe host doctor claude-code"

echo
echo "--- Node and npm ---"
if command -v node >/dev/null 2>&1; then
    node --version
else
    echo "node: missing"
fi

if command -v npm >/dev/null 2>&1; then
    npm --version
else
    echo "npm: missing"
fi

echo
echo "--- Claude CLI ---"
if command -v claude >/dev/null 2>&1; then
    command -v claude
    echo "Claude found in PATH"
else
    echo "Claude not in PATH"
fi

echo
echo "--- Claude Doctor ---"
if command -v claude >/dev/null 2>&1; then
    if claude doctor; then
        echo "claude doctor completed"
    else
        echo "claude doctor failed"
    fi
else
    echo "Skipped because claude is not installed"
fi

echo
echo "--- API Key Status ---"
if [[ -n "${ANTHROPIC_API_KEY:-}" ]]; then
    echo "ANTHROPIC_API_KEY is set"
else
    echo "ANTHROPIC_API_KEY is not set"
fi

echo
echo "--- MCP Servers ---"
if command -v claude >/dev/null 2>&1; then
    claude mcp list || echo "Unable to list MCP servers"
else
    echo "Skipped because claude is not installed"
fi

echo
echo "--- Config Paths ---"
if [[ -f "${HOME}/.claude.json" ]]; then
    echo "Found legacy config: ${HOME}/.claude.json"
else
    echo "Missing legacy config: ${HOME}/.claude.json"
fi

if [[ -f "${HOME}/.claude/settings.json" ]]; then
    echo "Found settings: ${HOME}/.claude/settings.json"
else
    echo "Missing settings: ${HOME}/.claude/settings.json"
fi

echo
echo "=== Health Check Complete ==="
