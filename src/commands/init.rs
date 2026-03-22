use crate::ecosystem;
use anyhow::Result;

pub fn run(client: Option<&str>) -> Result<()> {
    ecosystem::run_ecosystem(client, 0)
}
