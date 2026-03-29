use anyhow::Result;
use spore::editors::{self, McpServer};
use std::path::PathBuf;

pub(crate) use spore::editors::Editor;

pub(super) fn detect_editors() -> Vec<Editor> {
    editors::detect()
}

pub(super) fn config_path(editor: Editor) -> Result<PathBuf> {
    editors::config_path(editor).map_err(|err| anyhow::anyhow!(err.to_string()))
}

pub(super) fn register_mcp_servers(editor: Editor, servers: &[McpServer<'_>]) -> Result<()> {
    editors::register_mcp_servers(editor, servers).map_err(|err| anyhow::anyhow!(err.to_string()))
}
