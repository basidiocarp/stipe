# Stipe

Ecosystem installer and manager for Basidiocarp. Downloads binaries, registers
MCP servers with supported hosts, installs hook adapters, and runs cross-tool
health checks.

Named after the fungal stipe, the supporting stalk that holds the rest of the
fruiting body together.

Part of the [Basidiocarp ecosystem](https://github.com/basidiocarp).

---

## The Problem

Shared onboarding, repair, and host setup are easy to bolt onto unrelated tools,
but that makes each tool harder to reason about and turns install policy into a
scattered maintenance problem.

## The Solution

Stipe centralizes ecosystem management. It installs managed tools, registers
MCP servers for supported hosts, wires up hooks and notifications, and runs the
doctor flows that check whether all of that still works.

---

## The Ecosystem

| Tool | Purpose |
|------|---------|
| **[stipe](https://github.com/basidiocarp/stipe)** | Ecosystem installer and manager |
| **[cap](https://github.com/basidiocarp/cap)** | Web dashboard for the ecosystem |
| **[cortina](https://github.com/basidiocarp/cortina)** | Lifecycle signal capture and session attribution |
| **[hyphae](https://github.com/basidiocarp/hyphae)** | Persistent agent memory |
| **[lamella](https://github.com/basidiocarp/lamella)** | Skills, hooks, and plugins for coding agents |
| **[mycelium](https://github.com/basidiocarp/mycelium)** | Token-optimized command output |
| **[rhizome](https://github.com/basidiocarp/rhizome)** | Code intelligence via tree-sitter and LSP |
| **[spore](https://github.com/basidiocarp/spore)** | Shared transport and editor primitives |
| **[volva](https://github.com/basidiocarp/volva)** | Execution-host runtime layer |

> **Boundary:** `stipe` owns managed tool inventory, install profiles, doctor
> severity, and cross-tool orchestration. `spore` owns the reusable editor and
> transport primitives underneath that policy.

---

## Quick Start

```bash
curl -fsSL https://raw.githubusercontent.com/basidiocarp/.github/main/install.sh | sh
stipe doctor
stipe update --all
```

```bash
stipe install --all
stipe init
stipe host list
stipe host setup codex
stipe host doctor codex
```

---

## How It Works

```text
Operator               Stipe                          Host and tools
────────               ─────                          ──────────────
install profile  ─►    release + inventory     ─►    managed binaries
run init         ─►    host setup              ─►    MCP, hooks, DB setup
run doctor       ─►    host-aware checks       ─►    repair guidance
```

1. Install tools: download the selected managed binaries from release sources.
2. Initialize hosts: register MCP servers, hooks, notifications, and local databases.
3. Inspect hosts: show detected hosts and their current setup state.
4. Diagnose drift: report missing config, broken hook wiring, and stale setup.
5. Update the stack: refresh managed tools to newer versions.

---

## Install Profiles

| Profile | Tools |
|---------|-------|
| `minimal` | `mycelium`, `hyphae` |
| `standard` | `mycelium`, `hyphae`, `rhizome`, `cortina`, `lamella` |
| `claude-code` | `mycelium`, `hyphae`, `rhizome`, `cortina` |
| `codex` | `mycelium`, `hyphae`, `rhizome` |
| `cursor` | `mycelium`, `hyphae`, `rhizome` |
| `full` | `mycelium`, `hyphae`, `rhizome`, `cortina`, `lamella`, `cap`, `canopy`, `volva` |
| `developer-tools` | advisory third-party tooling hints only |

---

## What Stipe Owns

- Managed tool inventory and release mapping
- Host setup and MCP registration policy
- Approval memory and runtime policy for repeated host setup decisions
- Hook and notification adapter installation
- Ecosystem and host-specific doctor flows

## Foundation Boundary

Stipe is a policy layer over shared primitives. It owns host setup, install,
doctor, repair, provider health, registration policy, and the approval memory
or runtime policy state that explains why repeated setup actions are allowed or
denied.

It does not own authoring, packaging, or durable memory semantics. Those
concerns stay in the owning tools and shared helpers, and shared low-level
behavior should remain shared instead of being reimplemented locally.

## Approval Memory And Runtime Policy

Stipe keeps a narrow approval-memory model for repeated runtime-facing setup.
The first cut is intentionally small:

- remembered approvals or deny decisions are explicit records, not hidden booleans
- policy scope is ordered `project -> user`
- decision provenance is recorded so doctor output can say whether a rule came
  from an operator profile action, an operator-edited policy file, or imported
  config

This policy layer is host-respecting. Stipe can remember operator intent and
surface active policy, but it should not silently invent new approvals on the
user's behalf.

## What Stipe Does Not Own

- Shell filtering: handled by `mycelium`
- Memory storage: handled by `hyphae`
- Code intelligence: handled by `rhizome`
- Shared editor primitives: handled by `spore`

---

## Key Features

- Multi-host setup: supports host-specific setup and doctor surfaces.
- Managed profiles: install the right tool set for a chosen runtime.
- Drift detection: checks for missing registrations and broken hook coverage.
- Advisory developer tools: can report useful CLI tools without making them part of managed health.

## Terminal UI Boundary

If `stipe` ever adopts `ratatui`, the only bounded candidate flow is `stipe doctor`.
That surface already aggregates the most operator-facing status data, so it is the
one place where a richer dashboard could plausibly earn its maintenance cost.

For now, `stipe` keeps `doctor`, install profile selection, and host setup previews
on the existing `dialoguer` and `indicatif` model. A fullscreen TUI would add a new
rendering stack on top of a command surface that is already snapshot-tested, JSON-capable,
and easy to run in CI or over SSH. `ratatui` should not spread into install, host setup,
or other flows until one bounded `doctor` dashboard proves clearly better than the
plain CLI output it would replace.

---

## Architecture

```text
stipe/
├── src/commands/   install, init, doctor, update, and status flows
├── src/ecosystem/  shared inventory and policy
├── docs/           lightweight repo-local documentation index
├── scripts/claude/ fallback Claude-specific helpers
├── stipe/src/      CLI entry point
└── CHANGELOG.md    release history
```

```text
stipe install [--all] [--profile <name>] [tools...]
stipe host list
stipe host setup <host>
stipe host doctor [host]
stipe init
stipe doctor
stipe update --all
```

---

## Documentation

- [README.md](README.md): command surface, profiles, and setup behavior
- [CHANGELOG.md](CHANGELOG.md): release history
- [ROADMAP.md](ROADMAP.md): planned work
- [docs/README.md](docs/README.md): repo-local documentation index
- [scripts/claude/README.md](scripts/claude/README.md): fallback Claude-specific helper scripts

## Development

```bash
cargo build --release
cargo nextest run
cargo test
cargo clippy
cargo fmt
```

- Prefer `cargo nextest run` for the normal test loop.
- Keep `criterion` out of scope here until a concrete hot path is named.
- Use whole-command timing when install, init, or doctor flows feel slow, for
  example `time cargo run -- doctor --deep`.

## Logging

Stipe writes diagnostic logs to stderr through Spore's shared logger so command
output stays readable.

- Use `STIPE_LOG` for repo-specific logging, for example
  `STIPE_LOG=stipe=debug stipe doctor --deep`.
- `RUST_LOG` still works as the broader Rust fallback, but `STIPE_LOG` is the
  intended operator knob for this binary.
- Default runtime logging is `warn`, with lifecycle span events enabled so
  normal operator runs emit shared tracing boundaries without forcing `debug`.
- Logging is separate from Stipe's normal CLI output: human-readable status,
  install, and doctor output still goes to stdout, while diagnostics and tracing
  stay on stderr.
- `stipe init --json` keeps stdout reserved for the final JSON payload even when
  it needs to apply ecosystem setup first.

## License

MIT
