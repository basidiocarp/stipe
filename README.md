# Stipe

Ecosystem installer and manager for Basidiocarp. Downloads binaries, registers MCP servers with your editor, wires up hook and notification adapters, and runs health checks. One binary replaces the shell scripts and the 4,000 lines of ecosystem management that used to live in Mycelium.

Named after the mushroom's stem—the structural support connecting all parts.

Part of the [Basidiocarp ecosystem](https://github.com/basidiocarp).

## Quick Start

```bash
curl -fsSL https://raw.githubusercontent.com/basidiocarp/.github/main/install.sh | sh
```

The bootstrap script installs all ecosystem tools (stipe, mycelium, hyphae, rhizome, cortina) and configures your editor's MCP servers. After that, use stipe to manage updates and health checks:

```bash
stipe doctor       # verify everything is configured
stipe update --all # update to latest versions
```

To install individual tools later or on a fresh machine where you already have stipe:

```bash
stipe install --all
stipe init
stipe host codex
```

Install profiles:

- `minimal` installs `mycelium`
- `claude-code` installs `mycelium`, `hyphae`, `rhizome`, and `cortina`
- `codex` installs `mycelium`, `hyphae`, and `rhizome` for Codex host mode
- `cursor` installs `mycelium`, `hyphae`, and `rhizome`
- `full-stack` installs all ecosystem tools

## Commands

```
stipe install [--all] [--profile <name>] [--dry-run] [tools...]     Download tools from GitHub releases
stipe host <claude-code|codex|cursor> [--dry-run]                   Install and initialize a named host mode
stipe init [--client <name>] [--dry-run]                            Register MCP servers, install hooks and notify adapters, init databases
stipe doctor                                                        Health check across the full stack
stipe update [--all] [--check]                                      Update tools to latest versions
stipe status                                                        Show installed tools and versions
stipe uninstall [--all] [--dry-run] [tools...]                      Remove tools and configuration
```

## What `init` Does

1. Discovers installed tools via spore
2. Registers Hyphae and Rhizome as MCP servers with your editor
3. Configures Codex host mode by pointing `~/.codex/config.toml` at Hyphae's notify adapter
4. Installs Cortina hooks (PreToolUse, PostToolUse, Stop) for Claude Code
5. Creates the Hyphae database if missing
6. Patches CLAUDE.md with ecosystem instructions

Supports Claude Code, Codex CLI, Cursor, Windsurf, Cline, Continue, and Claude Desktop.

`stipe doctor` now also checks for setup drift by looking for MCP client config files that are missing `hyphae` or `rhizome` registrations, plus Codex notify adapter coverage.

## Why a Separate Tool

Mycelium compresses command output. That's its job. Ecosystem management was bolted on via `init --ecosystem` and grew into something unrelated to filtering. Stipe takes that responsibility so each tool stays focused.

## Development

```bash
cargo build --release
cargo test
cargo clippy
cargo fmt
```

## License

MIT
