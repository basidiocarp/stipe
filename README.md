# Stipe

Ecosystem installer and manager for Basidiocarp. Downloads binaries, registers MCP servers with your editor, wires up hook and notification adapters, and runs health checks across multiple hosts and platforms. One binary replaces the shell scripts and the 4,000 lines of ecosystem management that used to live in Mycelium.

Stipe now uses a shared tool registry plus a host inventory model: tool metadata is centralized in one place, each host gets its own setup and doctor flow, and shared tool state stays global.

Boundary note: `spore` stays responsible for editor primitives such as detection, config paths, MCP config writes, and editor capability differences. `stipe` stays responsible for ecosystem policy such as managed tool inventory, install profiles, doctor severity, release mapping, and cross-tool orchestration.

Named after the mushroom's stem—the structural support connecting all parts.

Part of the [Basidiocarp ecosystem](https://github.com/basidiocarp).

## Quick Start

```bash
curl -fsSL https://raw.githubusercontent.com/basidiocarp/.github/main/install.sh | sh
```

The bootstrap script installs all ecosystem tools (stipe, mycelium, hyphae, rhizome, canopy, cortina) and configures your editor's MCP servers. `canopy` is optional coordination-runtime coverage and only part of the `full-stack` profile. After that, use stipe to manage updates and health checks:

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

`stipe host` is the multi-host surface. Use `stipe doctor` for the aggregate ecosystem view, then `stipe host doctor <host>` when you need to isolate one runtime.

Migration note:

- If you previously used `mycelium init --ecosystem` or `mycelium init --onboard`, use `stipe init` for shared ecosystem setup and MCP/bootstrap repair.
- If you previously used `mycelium init --client <host>` or are fixing one runtime at a time, use `stipe host setup <host>`.
- Older single-level `stipe setup <host>` usage has been removed. Use `stipe host setup <host>`.

Install profiles:

- `minimal` installs `mycelium`
- `claude-code` installs `mycelium`, `hyphae`, `rhizome`, and `cortina`
- `codex` installs `mycelium`, `hyphae`, and `rhizome` for Codex host support
- `cursor` installs `mycelium`, `hyphae`, and `rhizome`
- `full-stack` installs `mycelium`, `hyphae`, `rhizome`, `canopy`, and `cortina`
- `developer-tools` is advisory only: it checks recommended third-party CLI tools and prints package-manager install hints instead of installing them into the managed bin directory

## Commands

```
stipe install [--all] [--profile <name>] [--dry-run] [tools...]              Download tools from GitHub releases
stipe host list                                                              Show known hosts and whether they are detected/configured
stipe host setup <claude-code|codex|cursor> [--scope <scope>] [--dry-run]   Install and initialize a named host
stipe host doctor [<claude-code|codex|cursor>] [--json]                      Check one host, or all managed hosts, independently
stipe init [--client <name>] [--scope <scope>] [--dry-run]                   Register MCP servers, install hooks and notify adapters, init databases
stipe doctor [--developer]                                                   Health check across the full stack and installed host config, optionally including advisory developer tools
stipe update [--all] [--check]                                               Update tools to latest versions
stipe status                                                                 Show installed tools and versions
stipe uninstall [--all] [--dry-run] [tools...]                               Remove tools and configuration
```

This first multi-host slice focuses on shared host descriptors, inventory, and per-host setup/doctor flows. Platform-aware path and shell differences will expand from here rather than landing in one pass.

## What `init` Does

1. Discovers installed tools via the shared `stipe` tool registry
2. Registers Hyphae and Rhizome as MCP servers with your editor
3. Installs the Codex notify adapter by adding Hyphae's notify command to `~/.codex/config.toml` or `.codex/config.toml`
4. Installs Cortina hooks (PreToolUse, PostToolUse, Stop) plus `statusLine.command = "cortina statusline"` in `~/.claude/settings.json`, `.claude/settings.json`, or `.claude/settings.local.json`
5. Creates the Hyphae database if missing
6. Patches CLAUDE.md with ecosystem instructions

Supports Claude Code, Codex CLI, Cursor, Windsurf, Cline, Continue, and Claude Desktop.

`stipe doctor` checks for setup drift by looking for MCP client config files that are missing `hyphae` or `rhizome` registrations, plus Codex notify and Claude hook coverage. Optional tools like `canopy` are surfaced without failing the overall doctor report when absent. Platform-specific config paths and shell guidance are expected to vary across macOS, Linux, and Windows, so host repair advice should stay tied to the detected platform rather than a single shell assumption. Shared editor mechanics should continue to land in `spore`; `stipe` should consume those primitives rather than duplicating them.

`stipe doctor --developer` adds an advisory developer-tools section. That surface checks third-party CLI utilities such as `jq`, `fd`, `shellcheck`, `tokei`, `bat`, `difftastic`, `just`, `cargo-nextest`, and related workflow tools, but it does not make them part of ecosystem health and it does not install or update them. Use `stipe install --profile developer-tools` when you want a package-manager hint list for the supported developer-tool tier set.

Temporary Claude-specific shell helpers migrated from Lamella live under
`scripts/claude/`. Treat them as fallback utilities while `stipe doctor`,
`stipe host doctor claude-code`, and future host repair flows absorb the
remaining manual recovery cases.

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
