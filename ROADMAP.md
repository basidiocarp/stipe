# Stipe Roadmap

This page is the Stipe-specific backlog. The workspace [ROADMAP.md](../ROADMAP.md) keeps the ecosystem sequencing, and [MASTER-ROADMAP.md](../MASTER-ROADMAP.md) keeps the cross-repo summary.

## Recently Shipped

- Multi-host `host list`, `host setup`, and `host doctor`.
- Shared platform-aware install, update, and uninstall path handling.
- Shared managed-tool registry for install, update, status, uninstall, doctor, and ecosystem status.
- Explicit optional `canopy` coverage in `stipe doctor` without treating it as a required host prerequisite.
- Better separation between host orchestration and tool inventory in `stipe`, with `spore` staying focused on shared editor/config mechanics.
- More portable repair guidance and config-path handling across macOS, Linux, and Windows.
- Better convergence with `spore` for editor detection and MCP registration.
- A narrower `spore` boundary: editor primitives stay in `spore`, while tool policy and orchestration stay in `stipe`.

## Next

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
