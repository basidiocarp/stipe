# Stipe Roadmap

This page is the Stipe-specific backlog. The workspace [ROADMAP.md](../ROADMAP.md) keeps the ecosystem sequencing, and [MASTER-ROADMAP.md](../MASTER-ROADMAP.md) keeps the cross-repo summary.

## Recently Shipped

- Multi-host `host list`, `host setup`, and `host doctor`.
- Shared platform-aware install, update, and uninstall path handling.
- Better separation between host orchestration in `stipe` and editor/config mechanics in `spore`.
- More portable repair guidance and config-path handling across macOS, Linux, and Windows.
- Better convergence with `spore` for editor detection and MCP registration.

## Next

### Install profiles

Add install profiles such as `minimal`, `claude-code`, `codex`, `cursor`, and `full-stack`.

### Drift detection and repair

Add richer config drift detection plus more direct repair flows.

### Safer operator flows

Add `--dry-run`, rollback, and safer install, init, update, and uninstall behavior.

### Bootstrap parity

Finish the platform-specific bootstrap story so Windows, macOS, and Linux have equally clear top-level setup.

## Later

### Release channels

Add stable, canary, and pinned release channels.

### Machine migration

Add machine bootstrap import and export for moving full setups between devices.

### Health automation

Add scheduled health checks and optional auto-repair.

### Host repair reuse

Add more reusable per-host adapter repair flows as the supported host set grows.

## Research

### Drift watcher

Explore a long-running local doctor or drift-watcher daemon once the current one-shot repair flows are solid.
