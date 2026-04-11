//! ASCII art banner for interactive mode.

use colored::Colorize;
use std::io::IsTerminal;

const MUSHROOM: &str = r"
⠀⠀⠀⠀⠀⠀⠀⠀⣀⣤⣤⡶⢶⣶⣶⣦⣤⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⣀⠴⠛⠛⠉⠄⠀⠀⠀⠀⠀⠀⠀⠈⠑⠒⣤⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⣤⡊⠁⣀⣤⣄⣤⣤⣤⣀⣤⣤⣤⣀⣤⣠⣶⣠⣀⠉⠢⡀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⡠⠋⢀⣠⣴⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣶⣿⡄⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠐⣡⣴⣿⣿⣿⣿⣿⣿⣿⣿⣿⡟⡹⣽⣿⣿⣿⣿⣿⣿⣿⣿⣯⣿⣟⣀⣠⡤⠶⢄⡀⠲⢄⠀⠀⠀⠀⠀⠀
⢹⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣯⡴⠶⣾⣿⣿⣿⣿⡿⠛⠛⠉⠉⠁⠀⠉⠀⠀⠀⠀⠈⠉⠒⢷⣄⠀⠀⠀⠀
⠀⠙⠻⣿⣿⣿⣿⣿⣿⣿⣿⣇⠀⠀⠘⣿⡿⢿⣯⣶⣶⣶⣶⣠⣴⣂⠀⠀⠀⠀⠀⠀⠀⠀⠀⠙⣆⠀⠀⠀
⠀⠀⠀⠀⠀⠉⠉⠉⠛⠋⠉⠉⡀⠀⠀⠹⡀⠀⢿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣺⢶⣶⣤⣀⡀⠀⠀⠘⣆⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢧⠀⠀⠂⣧⠀⠀⠉⠛⠻⠿⣿⣿⣿⣿⠟⠉⠛⣾⣿⣿⣷⣶⣄⡀⠘⢆⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⠀⠀⠀⢸⡄⠀⠀⠀⠀⠀⠈⠉⢻⠏⠀⠀⢠⣿⣿⣿⣿⣿⣿⣿⣶⣼⡆
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⠀⠀⠀⠸⡇⠀⠀⠀⠀⠀⠰⣖⠾⢧⣴⣲⣿⡏⠉⠛⠛⠻⠿⠿⣿⠿⠃
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠈⠂⠄⠀⢄⣧⠀⠀⠀⠀⠀⠀⠈⢹⠷⣶⡿⣯⣵⡤⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣇⡀⢆⢠⣿⠀⠀⠀⠀⠀⠀⡰⠋⠠⣿⣷⣶⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠸⣷⠿⡿⢹⣿⣆⠀⠀⠀⠀⡰⠉⠉⢹⡟⠈⠁⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠠⣿⣖⠘⣾⣹⡇⠀⠀⠀⡸⠁⠀⠀⣸⠃⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⢸⣿⣨⣾⣿⣧⡀⠀⠀⣇⠐⠀⣸⠇⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠠⢛⣿⠿⣄⣾⢷⠦⠀⢸⡿⢧⣿⡿⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⡀⢀⣿⣟⡾⢿⣿⣷⢽⡍⢷⣯⣿⠓⢤⣒⣦⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⢠⣴⡶⣀⠀⠀⢸⡽⠂⢸⣿⣽⣻⣿⣤⣾⣿⣿⣿⣿⣄⡀⡸⠀⠀⠀⢀⣀⠀⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠈⠉⠫⡁⠀⢻⠿⢷⣤⣿⣿⣶⣿⣿⣿⣯⣿⣻⣿⣿⣶⣠⣳⠀⠀⠐⣿⡿⠄⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠀⠀⠘⠲⠞⠒⠛⠛⠛⠛⠛⠋⠻⠛⠓⠛⠛⠛⠛⠟⠛⠛⠓⠚⠓⠛⠂⠀⠀⠀⠀⠀⠀⠀
";

/// Print the banner if stdout is a terminal (interactive mode).
pub fn print_banner() {
    if !std::io::stdout().is_terminal() {
        return;
    }

    println!();
    for line in render_banner_lines(banner_color_profile_from_env()) {
        println!("{line}");
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BannerColorProfile {
    Plain,
    Ansi,
    TrueColor,
}

fn banner_color_profile_from_env() -> BannerColorProfile {
    let no_color = std::env::var_os("NO_COLOR").is_some();
    let term = std::env::var("TERM").ok();
    let colorterm = std::env::var("COLORTERM").ok();
    resolve_banner_color_profile(true, no_color, term.as_deref(), colorterm.as_deref())
}

fn resolve_banner_color_profile(
    is_terminal: bool,
    no_color: bool,
    term: Option<&str>,
    colorterm: Option<&str>,
) -> BannerColorProfile {
    if !is_terminal || no_color {
        return BannerColorProfile::Plain;
    }

    if colorterm
        .map(|value| value.to_ascii_lowercase())
        .is_some_and(|value| value.contains("truecolor") || value.contains("24bit"))
    {
        return BannerColorProfile::TrueColor;
    }

    if term
        .map(str::trim)
        .is_some_and(|value| !value.is_empty() && value != "dumb")
    {
        return BannerColorProfile::Ansi;
    }

    BannerColorProfile::Plain
}

fn render_banner_lines(profile: BannerColorProfile) -> Vec<String> {
    let palette = [
        (232, 191, 106),
        (216, 160, 88),
        (174, 124, 78),
        (123, 146, 93),
        (102, 128, 87),
    ];

    MUSHROOM
        .trim_matches('\n')
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line_count = MUSHROOM.trim_matches('\n').lines().count().max(1);
            let band = index * palette.len() / line_count;
            match profile {
                BannerColorProfile::Plain => line.to_string(),
                BannerColorProfile::Ansi => match band.min(palette.len() - 1) {
                    0 => line.bright_yellow().to_string(),
                    1 => line.yellow().to_string(),
                    2 => line.bright_red().to_string(),
                    3 => line.green().to_string(),
                    _ => line.bright_green().to_string(),
                },
                BannerColorProfile::TrueColor => {
                    let (r, g, b) = palette[band.min(palette.len() - 1)];
                    format!("\u{1b}[38;2;{r};{g};{b}m{line}\u{1b}[0m")
                }
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{BannerColorProfile, render_banner_lines, resolve_banner_color_profile};
    use colored::control;
    use std::sync::{Mutex, OnceLock};

    fn banner_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn test_render_banner_lines_preserves_shape_and_adds_truecolor() {
        let _guard = banner_test_lock().lock().unwrap();
        control::set_override(true);
        let plain = render_banner_lines(BannerColorProfile::Plain);
        let colored = render_banner_lines(BannerColorProfile::TrueColor);
        control::unset_override();

        assert_eq!(plain.len(), colored.len());
        assert!(colored.iter().any(|line| line.contains("\u{1b}[")));
        assert!(colored.iter().any(|line| line.contains("38;2;")));
    }

    #[test]
    fn test_render_banner_lines_ansi_fallback_avoids_truecolor_sequences() {
        let _guard = banner_test_lock().lock().unwrap();
        control::set_override(true);
        let lines = render_banner_lines(BannerColorProfile::Ansi);
        control::unset_override();

        assert!(lines.iter().any(|line| line.contains("\u{1b}[")));
        assert!(lines.iter().all(|line| !line.contains("38;2;")));
    }

    #[test]
    fn test_resolve_banner_color_profile_prefers_plain_for_non_interactive_or_no_color() {
        assert_eq!(
            resolve_banner_color_profile(false, false, Some("xterm-256color"), Some("truecolor")),
            BannerColorProfile::Plain
        );
        assert_eq!(
            resolve_banner_color_profile(true, true, Some("xterm-256color"), Some("truecolor")),
            BannerColorProfile::Plain
        );
    }

    #[test]
    fn test_resolve_banner_color_profile_uses_truecolor_when_available() {
        assert_eq!(
            resolve_banner_color_profile(true, false, Some("xterm-256color"), Some("truecolor")),
            BannerColorProfile::TrueColor
        );
        assert_eq!(
            resolve_banner_color_profile(true, false, Some("screen"), Some("24bit")),
            BannerColorProfile::TrueColor
        );
    }

    #[test]
    fn test_resolve_banner_color_profile_falls_back_to_basic_ansi() {
        assert_eq!(
            resolve_banner_color_profile(true, false, Some("xterm-256color"), None),
            BannerColorProfile::Ansi
        );
        assert_eq!(
            resolve_banner_color_profile(true, false, Some("dumb"), None),
            BannerColorProfile::Plain
        );
    }
}
