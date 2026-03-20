# Stipe

Ecosystem installer and manager for Basidiocarp. Named after the mushroom's stem — the structural support connecting all parts.

Part of the [Basidiocarp ecosystem](https://github.com/basidiocarp).

## What It Does

Stipe replaces the shell scripts (`install.sh`, `update.sh`) and the ecosystem management code in Mycelium (`init --ecosystem`). One binary handles installation, configuration, health checks, and updates for the entire ecosystem.

```bash
stipe install --all        # download mycelium, hyphae, rhizome, cortina
stipe init                 # register MCP servers, install hooks, init databases
stipe doctor               # check ecosystem health (aggregates all tool doctors)
stipe update --all         # update everything to latest
stipe status               # show installed tools and versions
stipe uninstall --all      # clean removal
```

## Why a Separate Tool?

Mycelium's job is token compression. Ecosystem management was bolted on via `init --ecosystem` and grew to ~4,000 lines of code unrelated to filtering. Stipe takes that responsibility so each tool stays focused:

| Tool | Job |
|------|-----|
| Mycelium | Filter command output |
| Hyphae | Persistent memory + RAG |
| Rhizome | Code intelligence |
| Cortina | Hook runner |
| Stipe | Install, configure, update, diagnose |

## Install

The bootstrap installs only Stipe. Stipe handles everything else.

```bash
curl -fsSL install.basidiocarp.dev | sh    # installs stipe
stipe install --all                         # installs the ecosystem
stipe init                                  # configures your editors
```

## Commands

```
stipe install [--all] [tools...]     Install tools (mycelium, hyphae, rhizome, cortina)
stipe update [--all] [--check]       Update tools to latest versions
stipe init [--client <name>]         Configure MCP clients, hooks, databases
stipe doctor                         Aggregate health check across all tools
stipe status                         Show installed tools and versions
stipe uninstall [--all] [tools...]   Remove tools and configuration
```

## What `stipe init` Does

1. Detects installed tools via spore discovery
2. Registers Hyphae and Rhizome MCP servers with detected editors
3. Installs Cortina hooks (PreToolUse, PostToolUse, Stop)
4. Initializes Hyphae database if missing
5. Patches CLAUDE.md with ecosystem instructions

Supports: Claude Code, Cursor, Windsurf, Cline, Continue, Claude Desktop.

## Development

```bash
cargo build --release
cargo test
cargo clippy
cargo fmt
```

## Status

Bootstrapped with command stubs. Implementation pending — see `.plans/` for roadmap.

## License

MIT
