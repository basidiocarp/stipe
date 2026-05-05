# Stipe Roadmap

This page is the Stipe-specific backlog. The workspace [ROADMAP.md](../docs/workspace/ROADMAP.md) keeps the ecosystem sequencing and cross-repo priorities.

## Recently Shipped

- Stipe now supports multi-host `host list`, `host setup`, and `host doctor` flows. The installer story is no longer tied to one selected host or one machine shape.
- Install, update, uninstall, doctor, and ecosystem status all share a managed-tool registry and platform-aware path handling. That makes the tool inventory much more predictable across operating systems.
- The Stipe and Spore boundary is clearer than it used to be. Spore stays focused on shared editor and config primitives, while Stipe stays on orchestration, policy, repair behavior, and other host-facing decisions that should not be reimplemented in local helpers.
- Optional `canopy` coverage now appears in doctor flows without forcing Canopy to be treated as a required prerequisite for the rest of the stack.
- Portable repair guidance is broader across macOS, Linux, and Windows, and Lamella's remaining Claude host shell helpers now live in Stipe as a temporary bridge instead of hiding inside the packaging layer.

## Next

### Legacy burn-down

As of 0.5.0, legacy CLI compatibility shims have been removed. Stipe now uses `stipe init` and `stipe host setup` consistently. This work is largely complete.

### Drift detection and repair

The next priority is better config drift detection and more direct repair flows. Operators should be able to see what drifted, why it matters, and what safe repair path Stipe can offer.

### Safer operator flows

`--dry-run` is implemented for install, init, uninstall, and host setup. Next priority is rollback and recovery after partial installs to improve recoverability of infrastructure operations.

### Recoverability and machine migration

`stipe setup` sequences install + init in one step for new machine bootstrap, and `install.sh` provides the POSIX curl bootstrap entry point. Bootstrap parity across platforms is largely complete. Remaining work is expanding recoverability and supporting clean machine migration workflows.

## Later

### Release channels

Stable, canary, and pinned version channels make sense once the base install and repair path is settled. Release channels add value when users can already trust what a normal install does.

### Machine migration

Bootstrap import and export should eventually let a full setup move cleanly between devices. That becomes more useful once the current host inventory and repair flows are routine.

### Health automation

Scheduled health checks and optional auto-repair belong here after the one-shot doctor and repair flows are solid. Automation is only helpful if the manual path is already trustworthy.

### Host repair reuse

As the supported host set grows, Stipe should collect more reusable per-host adapter repair flows instead of growing one-off scripts for each integration.

## Research

### Drift watcher

A long-running local drift watcher may be worth building later. The open question is whether it would catch enough real failures to justify the extra runtime surface beyond the current one-shot doctor model.
