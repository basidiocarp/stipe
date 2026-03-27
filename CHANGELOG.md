# Changelog

## [0.4.4] - 2026-03-27

### Added

- **Host adapter installation**: `stipe init` and `stipe host setup` now install real Claude Code and Codex host adapters instead of only describing the repair steps.

### Changed

- **Claude host validation**: `stipe host doctor` and `stipe doctor` now treat Cortina hook coverage as part of Claude Code host readiness.
- **Codex host validation**: Codex notify coverage now accepts the expected Hyphae entries even when users keep additional notify commands in the same config.
- **Targeted host setup**: `stipe init --client codex` no longer opportunistically configures unrelated host adapters when a specific host was requested.

## [0.4.3] - 2026-03-26

### Fixed

- **Published Spore dependency**: Release and CI builds now resolve `spore` from the tagged git dependency instead of a workspace-only local path override, which fixes tagged builds on GitHub Actions and other non-workspace environments.

## [0.4.2] - 2026-03-26

### Added

- **Multi-host commands**: `stipe host list`, `stipe host setup <host>`, and `stipe host doctor [host]` now expose explicit per-host setup and health flows.
- **Shared install path resolver**: install, update, and uninstall now share one platform-aware local bin-dir resolver, including Windows-friendly fallback locations.

### Changed

- **Host inventory planning**: `stipe init` and `stipe doctor` now reuse shared host inventory and host-health models instead of special-casing Codex-only logic.
- **Shared editor convergence**: `stipe` now delegates more editor detection and config mutation work to `spore`, while keeping `Claude Code`, `Cline`, and `Continue` as explicit local exceptions.
- **Platform-aware host guidance**: host config paths and repair text now render through shared helpers instead of Unix-shaped hardcoded strings.

## [0.4.1] - 2026-03-23

### Fixed

- **GitHub updater 403s**: `stipe install` and `stipe update` now send a proper GitHub `User-Agent`, use `GH_TOKEN` / `GITHUB_TOKEN` when available, and report rate-limit failures more clearly instead of surfacing opaque `403 Forbidden` errors.

## [0.4.0] - 2026-03-23

### Added

- **Codex install profile**: `stipe install --profile codex` now installs the core Codex-oriented local stack.
- **Codex-aware repair guidance**: `stipe doctor` and `stipe init --dry-run --json` now surface Codex notify adapter setup as a first-class repair path.

### Changed

- **Host adapter terminology**: CLI and README guidance now distinguish MCP registration, Claude hooks, and Codex notifications more explicitly instead of routing everything through Claude-centric wording.

## [0.3.0] - 2026-03-22

### Added

- **Structured repair reports**: `stipe doctor --json` and `stipe init --dry-run --json` now emit machine-readable status, planned steps, and repair actions for tooling such as Cap.
- **Shared repair actions**: Health checks and init planning now point at concrete repair commands instead of plain text guidance.

### Changed

- **CLI repair output**: Human-readable doctor output now lists recommended repair commands when checks fail.

## [0.2.0] - 2026-03-22

### Added

- **Install profiles**: `minimal`, `claude-code`, `cursor`, and `full-stack` provide faster setup for common environments.
- **Dry-run support**: `stipe install`, `stipe init`, and `stipe uninstall` can now print the planned work before making changes.
- **Config drift checks**: `stipe doctor` now checks supported MCP client config files for missing Hyphae and Rhizome registrations.

### Changed

- **Onboarding guidance**: Documentation and health-check output now steer users toward profile-based installs and `stipe init` for repair.

## [0.1.3] - 2026-03-22

### Fixed

- **Tarball extraction safety**: Replaced `.unwrap()` on `file_name()` with `if let` (prevents panic on malformed archives).
- **Version error handling**: Propagates errors instead of using `"unknown"` sentinel that broke update comparison logic.
- **Direct install call**: `stipe update` calls `install_tool` directly instead of spawning a subprocess (eliminates PATH hazard and re-install skip).
- **Uninstall implemented**: Was a no-op stub, now removes binaries from `~/.local/bin/`.
- **Doctor error detail**: Health check failures include the actual error message instead of generic text.
- **Init error propagation**: `register_mcp_for_editor` returns `Result<()>`, non-UTF-8 path errors surface properly.
- **Version stderr**: Update command includes stderr in error messages for diagnostics.

### Changed

- **Platform key**: Returns `&'static str` (compile-time constant) instead of heap-allocated `String`.
- **Asset lookup**: Returns `&ReleaseAsset` reference instead of cloning.
- **TOOLS constant**: Centralized tool metadata replaces scattered string literals.
- **Shared HTTP client**: Single `reqwest::Client` created once and passed to all network functions.
- **Spore v0.4.0**: Updated for `SporeError` migration.

## [0.1.1] - 2026-03-22

### Added

- Interactive tool selection via `dialoguer::MultiSelect` during `stipe install`
- Download progress bars via `indicatif` with transfer speed and ETA
- Editor detection via `spore::editors::detect()` during `stipe init`
- MCP server registration via `spore::editors::register_mcp_server()` for all detected editors

### Changed

- Bumped spore dependency from v0.2.0 to v0.3.1
- `init` now uses shared editor detection instead of custom Claude Code-only logic
- `install` defaults to interactive multi-select when no tools specified

## [0.1.0] - 2026-03-20

Initial release.

- `stipe install` downloads tools from GitHub releases (Mycelium, Hyphae, Rhizome, Cortina)
- `stipe init` registers MCP servers, installs hooks, initializes Hyphae database
- `stipe doctor` runs health checks across the ecosystem
- `stipe update` checks and installs latest versions
- `stipe status` shows installed tools and versions
- `stipe uninstall` removes tools and configuration
- Supports 6 editors: Claude Code, Cursor, Windsurf, Cline, Continue, Claude Desktop
- Uses spore for tool discovery and platform paths
