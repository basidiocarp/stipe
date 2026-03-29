pub use super::tool_registry::InstallProfile;

mod release;
mod runner;
mod selection;
#[cfg(test)]
mod tests;

pub(crate) use runner::{install_bin_dir, install_tool, run};

#[cfg(test)]
use release::{GitHubRelease, ReleaseAsset, extract_tarball, find_matching_asset, platform_key};
#[cfg(test)]
use selection::{format_install_preview, resolve_requested_tools};
