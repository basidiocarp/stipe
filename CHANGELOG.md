# Changelog

All notable changes to Stipe are documented in this file.

## [Unreleased]

## [0.5.11] - 2026-04-09

### Changed

- **Lighter release downloads**: Install, update, and GitHub-release fetch
  paths now use `ureq`, and interactive prompts are aligned on `dialoguer`
  `0.12`.
- **Docs structure**: The docs set now includes a central `docs/README.md` and
  plan index that match the current lowercase docs layout.

### Fixed

- **Compile-info drag**: Stipe no longer carries the blocking `reqwest` stack
  for release download paths, which reduces dependency weight in the CLI.

## [0.5.9] - 2026-04-08

### Changed

- **Foundation alignment**: README, roadmap, and maintainer guidance now
  describe Stipe's host-policy boundary more explicitly.
- **Module boundaries**: Large runtime-adjacent test coverage moved into
  focused `tests.rs` modules under `init`, `update`, `doctor`, and ecosystem
  status surfaces.

### Fixed

- **JSON operator flow**: `stipe init --json` now keeps stdout machine-clean
  while still surfacing setup failures accurately.
- **Setup diagnostics**: Host registration and Hyphae setup flows now report
  subprocess failures and stderr more reliably.

## [0.5.8] - 2026-04-08

### Changed

- **Shared logging rollout**: Stipe now initializes logging through Spore's
  app-aware `STIPE_LOG` path instead of relying on generic runtime setup.
- **Verification tracing**: Install, smoke-test, and MCP handshake subprocesses
  now emit shared tracing spans with workspace-aware context for faster failure
  localization during doctor and install flows.

### Fixed

- **Operator guidance**: Docs now distinguish debug logging from Stipe's normal
  CLI output.

## [0.5.7] - 2026-04-03

### Added

- **From-source install fallback**: `stipe install --from-source` can now build
  ecosystem tools with `cargo install` when prebuilt binaries are unavailable.
- **Install helper scripts**: Added supporting scripts for the from-source
  installation path.

## [0.5.4] - 2026-04-01

### Added

- **Deep runtime verification**: `stipe` now supports tiered binary checks with
  functional smoke tests and MCP initialize handshakes instead of relying on
  `--version` alone.
- **Doctor deep mode**: `stipe doctor --deep` now runs per-tool functional
  checks and MCP startup validation for managed MCP servers.

### Changed

- **Install verification**: Managed binary installs now run tool-specific smoke
  tests after the version probe and warn when runtime checks fail.
- **Shared verification plumbing**: Install, probing, doctor, and ecosystem MCP
  registration now reuse one timeout-backed runtime verification layer.

## [0.5.3] - 2026-04-01

### Fixed

- **Claude host setup completeness**: `stipe host setup claude-code` now
  installs Cortina's `statusLine.command` alongside the existing Claude hooks.
- **Live MCP startup checks**: `stipe doctor` now probes Hyphae and Rhizome
  with a real MCP initialize handshake.

## [0.5.2] - 2026-03-31

### Added

- **Developer tool audit tier**: `stipe install --profile developer-tools` now
  prints advisory package-manager guidance, and `stipe doctor --developer` adds
  a separate developer-tools section.

## [0.5.1] - 2026-03-31

### Added

- **Self-update command**: `stipe self update` can now fetch and replace the
  running binary from the published GitHub release artifact for the current
  platform.

### Fixed

- **Profile-aware updates**: `stipe update --profile <profile>` now updates the
  selected profile instead of failing CLI parsing.

## [0.5.0] - 2026-03-31

### Changed

- **Single setup owner**: Stipe is now the explicit owner of shared ecosystem
  setup, onboarding, host mutation, and repair flows.
- **Host-specific setup boundary**: `stipe init --client ...` and
  `stipe host setup ...` now stay scoped to the requested host.
- **Published Spore discovery**: Stipe now consumes the released Spore tool
  registry, including Canopy and Cortina discovery.

### Fixed

- **Legacy setup drift**: Compatibility paths that kept Mycelium and Stipe
  overlap alive were removed so the older dual-owner setup model does not
  reappear.

## [0.4.7] - 2026-03-29

### Changed

- **Shared tool inventory**: Install, update, status, uninstall, doctor, and
  ecosystem status now read from one centralized tool registry.
- **Command structure**: Large command modules such as `init`, `doctor`,
  `install`, and `host` were split into smaller planning, model, render, and
  execution modules.
- **Ecosystem workflow split**: `run_ecosystem` now builds context, renders
  status, and executes host and client configuration as separate steps.
- **CLI snapshot coverage**: Human-facing output for the main commands now has
  stable snapshot-style coverage.
- **Spore integration boundary**: Stipe now consumes Spore editor descriptors
  directly while keeping tool inventory and doctor semantics local.

### Fixed

- **Released Spore dependency**: Stipe now pins Spore to a released tag instead
  of a transient git revision.

## [0.4.6] - 2026-03-28

### Added

- **Canopy tool management**: `stipe install`, `update`, `status`, `uninstall`,
  and ecosystem summaries now include `canopy` as a managed binary.

### Changed

- **Full-stack profile**: `stipe install --profile full-stack` now includes
  `canopy` alongside `mycelium`, `hyphae`, `rhizome`, and `cortina`.
- **Inventory docs**: Help text and installation docs now list `canopy` in the
  managed tool inventory.

## [0.4.5] - 2026-03-27

### Changed

- **Rust quality gate**: `stipe` is now clippy-clean under
  `cargo clippy -p stipe --all-targets -- -D warnings`.

### Fixed

- **Claude hook install target**: `stipe init` now writes Cortina Claude hooks
  to the supported Claude settings files instead of mixing registration into
  `~/.claude.json`.
- **Scoped host adapter installs**: `stipe init` and `stipe host setup` now
  accept `--scope`, and Codex project installs can target `.codex/config.toml`.
- **Scoped health detection**: Host list and doctor flows now recognize Claude
  and Codex installs across supported user and project scopes.

## [0.4.4] - 2026-03-27

### Added

- **Host adapter installation**: `stipe init` and `stipe host setup` can now
  install real Claude Code and Codex host adapters instead of only describing
  repair steps.

### Changed

- **Claude host validation**: `stipe host doctor` and `stipe doctor` now treat
  Cortina hook coverage as part of Claude Code readiness.
- **Codex host validation**: Codex notify coverage now accepts the expected
  Hyphae entries even when users keep extra notify commands in the same config.
- **Targeted host setup**: `stipe init --client codex` no longer configures
  unrelated host adapters opportunistically.

## [0.4.3] - 2026-03-26

### Fixed

- **Published Spore dependency**: Release and CI builds now resolve Spore from
  the tagged git dependency instead of a workspace-only local override.

## [0.4.2] - 2026-03-26

### Added

- **Multi-host commands**: Added `stipe host list`, `stipe host setup <host>`,
  and `stipe host doctor [host]`.
- **Shared install path resolver**: Install, update, and uninstall now share
  one platform-aware local bin-dir resolver.

### Changed

- **Host inventory planning**: `stipe init` and `stipe doctor` now reuse shared
  host inventory and host-health models.
- **Shared editor convergence**: Stipe now delegates more editor detection and
  config mutation work to Spore while keeping local exceptions where needed.
- **Platform-aware host guidance**: Host config paths and repair text now
  render through shared helpers instead of Unix-shaped hardcoded strings.

## [0.4.1] - 2026-03-23

### Fixed

- **GitHub updater 403s**: `stipe install` and `stipe update` now send a proper
  GitHub `User-Agent`, use `GH_TOKEN` or `GITHUB_TOKEN` when available, and
  report rate-limit failures more clearly.

## [0.4.0] - 2026-03-23

### Added

- **Codex install profile**: `stipe install --profile codex` now installs the
  core Codex-oriented local stack.
- **Codex-aware repair guidance**: `stipe doctor` and `stipe init --dry-run
  --json` now surface Codex notify adapter setup as a first-class repair path.

### Changed

- **Host adapter terminology**: CLI and README guidance now distinguish MCP
  registration, Claude hooks, and Codex notifications more explicitly.

## [0.3.0] - 2026-03-22

### Added

- **Structured repair reports**: `stipe doctor --json` and
  `stipe init --dry-run --json` now emit machine-readable status, planned
  steps, and repair actions for tools such as Cap.
- **Shared repair actions**: Health checks and init planning now point at
  concrete repair commands instead of plain text guidance.

### Changed

- **CLI repair output**: Human-readable doctor output now lists recommended
  repair commands when checks fail.

## [0.2.0] - 2026-03-22

### Added

- **Install profiles**: Added `minimal`, `claude-code`, `cursor`, and
  `full-stack` profiles for common environments.
- **Dry-run support**: `stipe install`, `stipe init`, and `stipe uninstall` can
  now print planned work before making changes.
- **Config drift checks**: `stipe doctor` now checks supported MCP client
  config files for missing Hyphae and Rhizome registrations.

### Changed

- **Onboarding guidance**: Docs and doctor output now steer users toward
  profile-based installs and `stipe init` for repair.

## [0.1.3] - 2026-03-22

### Changed

- **Platform key and asset lookup**: `platform_key()` now returns a static str,
  and asset lookup now returns references instead of cloning.
- **Shared HTTP client**: Network paths now reuse one shared `ureq`-based GitHub client.
- **Centralized tool metadata**: `TOOLS` replaced scattered string literals.
- **Spore migration**: Stipe updated for the shared `SporeError` surface.

### Fixed

- **Tarball extraction safety**: Archive extraction no longer unwraps a missing
  filename.
- **Version error handling**: Update comparisons now propagate real errors
  instead of relying on an `"unknown"` sentinel.
- **Direct install call**: `stipe update` now calls `install_tool` directly
  instead of spawning a subprocess.
- **Uninstall implementation**: `stipe uninstall` now removes binaries instead
  of behaving like a stub.
- **Doctor error detail**: Health checks now include the underlying error
  message.
- **Init error propagation**: Non-UTF-8 path errors now surface correctly from
  MCP registration.
- **Version stderr capture**: Update failures now include stderr for better
  diagnostics.

## [0.1.1] - 2026-03-21

### Added

- **Interactive install selection**: `stipe install` now supports
  `dialoguer::MultiSelect`.
- **Download progress bars**: Downloads now show progress, transfer speed, and
  ETA through `indicatif`.
- **Shared editor detection**: `stipe init` now uses `spore::editors::detect()`
  and shared MCP registration helpers.

### Changed

- **Shared editor flow**: `init` no longer uses a custom Claude Code-only path.
- **Install defaults**: `install` now falls back to interactive multi-select
  when no tools are specified.
- **Spore dependency**: The release moved to the newer shared Spore runtime.

## [0.1.0] - 2026-03-20

### Added

- **Release installs**: `stipe install` downloads ecosystem tools from GitHub
  releases.
- **Host and MCP setup**: `stipe init` registers MCP servers, installs hooks,
  and initializes the Hyphae database.
- **Health checks**: `stipe doctor` runs ecosystem-wide health checks.
- **Version and status commands**: `stipe update`, `status`, and `uninstall`
  shipped in the initial release.
- **Editor coverage**: The first release supported Claude Code, Cursor,
  Windsurf, Cline, Continue, and Claude Desktop.
- **Shared path discovery**: Stipe used Spore for tool discovery and
  platform-aware paths from the start.
