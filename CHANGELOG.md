# Changelog

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
