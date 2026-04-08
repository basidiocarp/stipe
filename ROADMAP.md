# Stipe Roadmap

This page is the Stipe-specific backlog. The workspace [ROADMAP.md](../docs/ROADMAP.md) keeps the ecosystem sequencing and cross-repo priorities.

## Recently Shipped

- Stipe now supports multi-host `host list`, `host setup`, and `host doctor` flows. The installer story is no longer tied to one selected host or one machine shape.
- Install, update, uninstall, doctor, and ecosystem status all share a managed-tool registry and platform-aware path handling. That makes the tool inventory much more predictable across operating systems.
- The Stipe and Spore boundary is clearer than it used to be. Spore stays focused on shared editor and config primitives, while Stipe stays on orchestration, policy, repair behavior, and other host-facing decisions that should not be reimplemented in local helpers.
- Optional `canopy` coverage now appears in doctor flows without forcing Canopy to be treated as a required prerequisite for the rest of the stack.
- Portable repair guidance is broader across macOS, Linux, and Windows, and Lamella's remaining Claude host shell helpers now live in Stipe as a temporary bridge instead of hiding inside the packaging layer.

## Next

### Legacy burn-down

Stipe should remove temporary CLI compatibility shims once the ecosystem docs and local automation have fully converged on `stipe init` and `stipe host setup`. This is how the installer surface stops carrying old names and half-retired paths indefinitely.

### Drift detection and repair

The next priority is better config drift detection and more direct repair flows. Operators should be able to see what drifted, why it matters, and what safe repair path Stipe can offer.

### Safer operator flows

Install, init, update, and uninstall all need stronger `--dry-run`, rollback, and safety behavior. Stipe is infrastructure, so the bar is not just convenience; it is recoverability.

### Bootstrap parity

Windows, macOS, and Linux still need equally clear top-level bootstrap paths. The goal is one mental model for setup, even if the implementation details vary by platform.

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
