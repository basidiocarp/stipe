# Changelog

## [0.5.1] - 2026-03-31

### Added

- **Self-update command**: `stipe self update` can now check for and replace the running `stipe` binary using the published GitHub release artifact for the current platform.

### Fixed

- **Profile-aware updates**: `stipe update --profile <profile>` now works and updates the installed tools for the selected profile instead of failing CLI argument parsing.

## [0.5.0] - 2026-03-31

### Changed

- **Single setup owner**: Stipe is now the explicit owner of shared ecosystem setup, onboarding, host mutation, and repair flows across the active toolchain.
- **Host-specific setup boundary**: `stipe init --client ...` and `stipe host setup ...` now stay scoped to the requested host instead of leaking Claude-specific behavior into Codex paths.
- **Published Spore discovery**: Stipe now consumes the released `spore v0.4.6` tool registry, including Canopy and Cortina discovery.

### Fixed

- **Legacy setup drift**: Compatibility setup paths that kept Mycelium and Stipe overlap alive are removed, which prevents the old dual-owner setup model from reappearing.

## [0.4.7] - 2026-03-29

### Changed

- **Shared tool inventory**: install, update, status, uninstall, doctor, and ecosystem status now read from one centralized tool registry instead of maintaining separate hard-coded inventories.
- **Command structure**: large command modules such as `init`, `doctor`, `install`, and `host` were split into smaller planning, model, render, and execution modules for easier maintenance.
- **Ecosystem workflow split**: `run_ecosystem` now builds explicit context, renders status, and executes host/client configuration as separate steps instead of mixing planning and output in one module.
- **CLI snapshot coverage**: dry-run and human-facing output for `init`, `doctor`, `host`, `install`, `uninstall`, and ecosystem status now have stable snapshot-style coverage to make wording and repair-hint regressions easier to catch.
- **Spore integration boundary**: `stipe` now consumes `spore` editor descriptors directly, while keeping ecosystem policy such as tool inventory, release mapping, and doctor semantics local to `stipe`.

### Fixed

- **Released Spore dependency**: `stipe` now pins `spore` to released tag `v0.4.4` instead of a transient git revision.

## [0.4.6] - 2026-03-28

### Added

- **Canopy tool management**: `stipe install`, `stipe update`, `stipe status`, `stipe uninstall`, and the ecosystem summary now include `canopy` as a managed ecosystem binary.

### Changed

- **Full-stack profile**: `stipe install --profile full-stack` now installs `canopy` alongside `mycelium`, `hyphae`, `rhizome`, and `cortina`.
- **CLI and README inventory**: `stipe` help text and installation docs now list `canopy` in the managed tool inventory.

## [0.4.5] - 2026-03-27

### Fixed

- **Claude hook install target**: `stipe init` now writes Cortina Claude hooks to Claude settings files (`~/.claude/settings.json`, `.claude/settings.json`, or `.claude/settings.local.json`) instead of mixing hook registration into `~/.claude.json`.
- **Scoped host adapter installs**: `stipe init` and `stipe host setup` now accept `--scope`, and Codex project installs write to `.codex/config.toml` instead of only targeting the user config.
- **Scoped health detection**: `stipe host list`, `stipe host doctor`, and `stipe doctor` now recognize Claude and Codex adapter installs across the supported user and project scopes.

### Changed

- **Rust quality gate**: `stipe` is clippy-clean under `cargo clippy -p stipe --all-targets -- -D warnings`.

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
