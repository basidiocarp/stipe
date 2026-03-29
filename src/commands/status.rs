use anyhow::Result;

use super::tool_registry::{self, ToolProbe};

#[allow(clippy::unnecessary_wraps)]
pub fn run() -> Result<()> {
    println!("Basidiocarp Ecosystem Status");
    println!("{}", "─".repeat(40));

    for spec in tool_registry::status_specs() {
        match tool_registry::probe(spec) {
            ToolProbe::Installed(version) => {
                println!("  {:<12} v{:<10} installed", spec.name, version);
            }
            ToolProbe::Missing => {
                println!("  {:<12} {:<12} not installed", spec.name, "—");
            }
            ToolProbe::Broken => {
                println!("  {:<12} {:<12} installed but broken", spec.name, "!");
            }
        }
    }

    Ok(())
}
