use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

pub(crate) fn open_vscode_local(worktree_dir: &Path) -> Result<()> {
    let status = Command::new("code")
        .args(["--new-window"])
        .arg(worktree_dir)
        .status()
        .context("Failed to spawn `code`")?;
    if status.success() {
        Ok(())
    } else {
        bail!("`code` failed with status: {status}");
    }
}
