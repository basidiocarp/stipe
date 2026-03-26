use std::path::PathBuf;

fn local_bin_dir_from_parts(
    override_dir: Option<PathBuf>,
    home_dir: Option<PathBuf>,
    _data_local_dir: Option<PathBuf>,
    data_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(bin_dir) = override_dir {
        return Some(bin_dir);
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(dir) = _data_local_dir {
            return Some(dir.join("Basidiocarp").join("bin"));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = home_dir {
            return Some(home.join(".local").join("bin"));
        }
    }

    data_dir.map(|dir| dir.join("Basidiocarp").join("bin"))
}

pub fn local_bin_dir() -> Option<PathBuf> {
    local_bin_dir_from_parts(
        std::env::var_os("MYCELIUM_BIN_DIR").map(PathBuf::from),
        dirs::home_dir(),
        dirs::data_local_dir(),
        dirs::data_dir(),
    )
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
        let path = local_bin_dir_from_parts(
            Some(PathBuf::from("/tmp/custom-bin")),
            Some(PathBuf::from("/Users/test")),
            Some(PathBuf::from("/Users/test/AppData/Local")),
            Some(PathBuf::from("/Users/test/.local/share")),
        );

        assert_eq!(path, Some(PathBuf::from("/tmp/custom-bin")));
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_local_bin_dir_uses_home_on_non_windows() {
        let path = local_bin_dir_from_parts(
            None,
            Some(PathBuf::from("/Users/test")),
            None,
            Some(PathBuf::from("/Users/test/.local/share")),
        );

        assert_eq!(path, Some(PathBuf::from("/Users/test/.local/bin")));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn test_local_bin_dir_uses_data_local_on_windows() {
        let path = local_bin_dir_from_parts(
            None,
            Some(PathBuf::from("C:\\Users\\test")),
            Some(PathBuf::from("C:\\Users\\test\\AppData\\Local")),
            Some(PathBuf::from("C:\\Users\\test\\AppData\\Roaming")),
        );

        assert_eq!(
            path,
            Some(PathBuf::from(
                "C:\\Users\\test\\AppData\\Local\\Basidiocarp\\bin"
            ))
        );
    }
}
