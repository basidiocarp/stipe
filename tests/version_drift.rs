//! Runs the `version_drift` unit tests as part of `cargo test`.
//!
//! The `tools_diff`/`extract_tools` helpers are defined in `src/version_drift.rs` and
//! included by `build.rs` via `#[path]`. Cargo never runs tests defined inside a build
//! script, so this integration target includes the same module — an integration crate
//! always compiles with `cfg(test)`, so the module's `#[cfg(test)] mod tests` executes here.

#[path = "../src/version_drift.rs"]
mod version_drift;
