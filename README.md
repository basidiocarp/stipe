# Stipe

Ecosystem installer and manager for Basidiocarp. Downloads binaries, registers MCP servers with your editor, wires up hook and notification adapters, and runs health checks across multiple hosts and platforms. One binary replaces the shell scripts and the 4,000 lines of ecosystem management that used to live in Mycelium.

Stipe is moving from single-host modes to a host inventory model: each host gets its own setup and doctor flow, while shared tool state stays global.

Named after the mushroom's stem—the structural support connecting all parts.

Part of the [Basidiocarp ecosystem](https://github.com/basidiocarp).

## Quick Start

```bash
curl -fsSL https://raw.githubusercontent.com/basidiocarp/.github/main/install.sh | sh
```

The bootstrap script installs all ecosystem tools (stipe, mycelium, hyphae, rhizome, canopy, cortina) and configures your editor's MCP servers. After that, use stipe to manage updates and health checks:

```bash
stipe doctor       # verify everything is configured
stipe update --all # update to latest versions
```

To install individual tools later or on a fresh machine where you already have stipe:

```bash
stipe install --all
stipe init
stipe host list
stipe host setup codex
stipe host doctor codex
```

`stipe host` is the multi-host surface. Use `stipe doctor` for the aggregate ecosystem view, then `stipe host doctor <host>` when you need to isolate one runtime. `stipe host <mode>` remains as a hidden compatibility alias for now, but new automation should use `list`, `setup`, and `doctor` explicitly.

Install profiles:

- `minimal` installs `mycelium`
- `claude-code` installs `mycelium`, `hyphae`, `rhizome`, and `cortina`
- `codex` installs `mycelium`, `hyphae`, and `rhizome` for Codex host support
- `cursor` installs `mycelium`, `hyphae`, and `rhizome`
- `full-stack` installs `mycelium`, `hyphae`, `rhizome`, `canopy`, and `cortina`

## Commands

```
stipe install [--all] [--profile <name>] [--dry-run] [tools...]              Download tools from GitHub releases
stipe host list                                                              Show known hosts and whether they are detected/configured
stipe host setup <claude-code|codex|cursor> [--scope <scope>] [--dry-run]   Install and initialize a named host
stipe host doctor [<claude-code|codex|cursor>] [--json]                      Check one host, or all managed hosts, independently
stipe init [--client <name>] [--scope <scope>] [--dry-run]                   Register MCP servers, install hooks and notify adapters, init databases
stipe doctor                                                                 Health check across the full stack and installed host config
stipe update [--all] [--check]                                               Update tools to latest versions
stipe status                                                                 Show installed tools and versions
stipe uninstall [--all] [--dry-run] [tools...]                               Remove tools and configuration
```

This first multi-host slice focuses on shared host descriptors, inventory, and per-host setup/doctor flows. Platform-aware path and shell differences will expand from here rather than landing in one pass.

## What `init` Does

1. Discovers installed tools via spore
2. Registers Hyphae and Rhizome as MCP servers with your editor
3. Installs the Codex notify adapter by adding Hyphae's notify command to `~/.codex/config.toml` or `.codex/config.toml`
4. Installs Cortina hooks (PreToolUse, PostToolUse, Stop) in `~/.claude/settings.json`, `.claude/settings.json`, or `.claude/settings.local.json`
5. Creates the Hyphae database if missing
6. Patches CLAUDE.md with ecosystem instructions

Supports Claude Code, Codex CLI, Cursor, Windsurf, Cline, Continue, and Claude Desktop.

`stipe doctor` checks for setup drift by looking for MCP client config files that are missing `hyphae` or `rhizome` registrations, plus Codex notify and Claude hook coverage. Platform-specific config paths and shell guidance are expected to vary across macOS, Linux, and Windows, so host repair advice should stay tied to the detected platform rather than a single shell assumption.

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
