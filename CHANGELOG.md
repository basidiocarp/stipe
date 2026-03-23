# Changelog

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
