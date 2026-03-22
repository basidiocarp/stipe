# Stipe

Ecosystem installer and manager for Basidiocarp. Downloads binaries, registers MCP servers with your editor, wires up hooks, and runs health checks. One binary replaces the shell scripts and the 4,000 lines of ecosystem management that used to live in Mycelium.

Named after the mushroom's stem—the structural support connecting all parts.

Part of the [Basidiocarp ecosystem](https://github.com/basidiocarp).

## Quick Start

```bash
curl -fsSL https://raw.githubusercontent.com/basidiocarp/.github/main/install.sh | sh
stipe install --all
stipe init
```

The bootstrap installs only Stipe. `install --all` pulls Mycelium, Hyphae, Rhizome, and Cortina from GitHub releases. `init` detects your editor and configures everything.

## Commands

```
stipe install [--all] [tools...]     Download tools from GitHub releases
stipe init [--client <name>]         Register MCP servers, install hooks, init databases
stipe doctor                         Health check across the full stack
stipe update [--all] [--check]       Update tools to latest versions
stipe status                         Show installed tools and versions
stipe uninstall [--all] [tools...]   Remove tools and configuration
```

## What `init` Does

1. Discovers installed tools via spore
2. Registers Hyphae and Rhizome as MCP servers with your editor
3. Installs Cortina hooks (PreToolUse, PostToolUse, Stop)
4. Creates the Hyphae database if missing
5. Patches CLAUDE.md with ecosystem instructions

Supports Claude Code, Cursor, Windsurf, Cline, Continue, and Claude Desktop.

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
