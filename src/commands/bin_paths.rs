use std::path::{Path, PathBuf};

#[cfg(target_os = "windows")]
fn local_bin_dir_from_parts(
    override_dir: Option<&Path>,
    data_local_dir: Option<&Path>,
    data_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(bin_dir) = override_dir {
        return Some(bin_dir.to_path_buf());
    }

    if let Some(dir) = data_local_dir {
        return Some(dir.join("Basidiocarp").join("bin"));
    }

    data_dir.map(|dir| dir.join("Basidiocarp").join("bin"))
}

#[cfg(not(target_os = "windows"))]
fn local_bin_dir_from_parts(
    override_dir: Option<&Path>,
    home_dir: Option<&Path>,
    data_dir: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(bin_dir) = override_dir {
        return Some(bin_dir.to_path_buf());
    }

    if let Some(home) = home_dir {
        return Some(home.join(".local").join("bin"));
    }

    data_dir.map(|dir| dir.join("Basidiocarp").join("bin"))
}

pub fn local_bin_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        return local_bin_dir_from_parts(
            std::env::var_os("MYCELIUM_BIN_DIR")
                .as_deref()
                .map(Path::new),
            dirs::data_local_dir().as_deref(),
            dirs::data_dir().as_deref(),
        );
    }

    #[cfg(not(target_os = "windows"))]
    {
        local_bin_dir_from_parts(
            std::env::var_os("MYCELIUM_BIN_DIR")
                .as_deref()
                .map(Path::new),
            dirs::home_dir().as_deref(),
            dirs::data_dir().as_deref(),
        )
    }
}

pub fn local_bin_dir_display() -> String {
    local_bin_dir().map_or_else(
        || "your local bin directory".to_string(),
        |path| path.display().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_local_bin_dir_prefers_override() {
        let override_dir = PathBuf::from("/tmp/custom-bin");
        #[cfg(not(target_os = "windows"))]
        let path = local_bin_dir_from_parts(
            Some(override_dir.as_path()),
            Some(Path::new("/Users/test")),
            Some(Path::new("/Users/test/.local/share")),
        );
        #[cfg(target_os = "windows")]
        let path = local_bin_dir_from_parts(
            Some(override_dir.as_path()),
            Some(Path::new("/Users/test/AppData/Local")),
            Some(Path::new("/Users/test/AppData/Roaming")),
        );

        assert_eq!(path, Some(PathBuf::from("/tmp/custom-bin")));
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_local_bin_dir_uses_home_on_non_windows() {
        let home_dir = PathBuf::from("/Users/test");
        let data_dir = PathBuf::from("/Users/test/.local/share");
        let path =
            local_bin_dir_from_parts(None, Some(home_dir.as_path()), Some(data_dir.as_path()));

        assert_eq!(path, Some(PathBuf::from("/Users/test/.local/bin")));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_local_bin_dir_uses_data_local_on_windows() {
        let data_local_dir = PathBuf::from("C:\\Users\\test\\AppData\\Local");
        let data_dir = PathBuf::from("C:\\Users\\test\\AppData\\Roaming");
        let path = local_bin_dir_from_parts(
            None,
            Some(data_local_dir.as_path()),
            Some(data_dir.as_path()),
        );

        assert_eq!(
            path,
            Some(PathBuf::from(
                "C:\\Users\\test\\AppData\\Local\\Basidiocarp\\bin"
            ))
        );
    }
}
