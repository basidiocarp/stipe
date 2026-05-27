//! Shared path helpers for ecosystem tools.

use std::path::{Path, PathBuf};

/// Returns the canonical path to the hyphae database.
///
/// This path is shared between stipe's doctor tool and the ecosystem configuration.
/// It must remain in sync with hyphae's actual database location.
pub(crate) fn hyphae_db_path(data_dir: &Path) -> PathBuf {
    data_dir
        .join("basidiocarp")
        .join("hyphae")
        .join("hyphae.db")
}
