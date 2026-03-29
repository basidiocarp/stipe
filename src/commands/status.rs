use anyhow::Result;
use spore::{Tool, discover};
use std::process::Command;

fn canopy_version() -> Option<String> {
    let output = Command::new("canopy").arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().last())
        .filter(|version| version.contains('.'))
        .map(str::to_owned)
}

#[allow(clippy::unnecessary_wraps)]
pub fn run() -> Result<()> {
    println!("Basidiocarp Ecosystem Status");
    println!("{}", "─".repeat(40));

    for tool in Tool::all() {
        match discover(*tool) {
            Some(info) => {
                println!("  {:<12} v{:<10} installed", tool, info.version);
            }
            None => {
                println!("  {:<12} {:<12} not installed", tool, "—");
            }
        }
    }

    match canopy_version() {
        Some(version) => println!("  {:<12} v{:<10} installed", "canopy", version),
        None => println!("  {:<12} {:<12} not installed", "canopy", "—"),
    }

    Ok(())
}
