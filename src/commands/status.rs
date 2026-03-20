use anyhow::Result;
use spore::{Tool, discover};

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

    Ok(())
}
