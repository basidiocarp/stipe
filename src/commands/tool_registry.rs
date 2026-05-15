pub mod capability_registry;
mod model;
mod probe;
mod specs;
#[cfg(test)]
mod tests;

pub use capability_registry::{default_registry_path, write_capability_registry};
pub use model::{DoctorCoverage, InstallProfile, ToolProbe, ToolSpec};
pub use probe::{VerifyLevel, probe, probe_with_level, resolve_binary_path};
pub use specs::{
    all_specs, doctor_specs, ecosystem_specs, find, install_all_specs, installable_specs,
    release_archive_binaries, specs_for_profile, status_specs, uninstall_all_specs,
    update_all_specs,
};
