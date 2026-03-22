use anyhow::Result;
use crate::ecosystem;

pub fn run(client: Option<&str>) -> Result<()> {
    ecosystem::run_ecosystem(client, 0)
}
