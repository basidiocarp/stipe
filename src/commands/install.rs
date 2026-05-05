pub use super::tool_registry::InstallProfile;

mod profile_config;
mod profile_surface;
pub(crate) mod release;
mod runner;
mod selection;
pub(crate) mod skill_install;
#[cfg(test)]
mod tests;

pub(crate) use profile_config::{SavedInstallProfile, load_saved_profile, save_selected_profile};
pub(crate) use profile_surface::{ManualProfileMember, expected_profile_tools, manual_member};
pub(crate) use runner::{InstallOptions, install_bin_dir, install_tool, run, run_embedded_preview};
pub(crate) use skill_install::{SkillPackManifest, SkillVerifyStatus, install_skills};

#[cfg(test)]
use profile_config::{load_profile_from_path, save_profile_to_path};
#[cfg(test)]
use release::{GitHubRelease, ReleaseAsset, extract_tarball, find_matching_asset, platform_key};
#[cfg(test)]
use selection::{
    format_install_preview, render_embedded_profile_install_preview, render_install_preview,
    render_profile_install_preview, resolve_requested_tools, split_requested_tools,
};
