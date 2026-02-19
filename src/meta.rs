use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct AgentMeta {
    #[serde(default)]
    pub(crate) branch_name: Option<String>,
}

fn agent_meta_path(agent_name: &str) -> Result<PathBuf> {
    let rel = format!("pc/agents/{agent_name}.json");
    let output = Command::new("git")
        .args(["rev-parse", "--git-path", &rel])
        .output()
        .context("Failed to run git rev-parse --git-path")?;
    if !output.status.success() {
        bail!("git rev-parse --git-path failed");
    }
    let s = String::from_utf8(output.stdout).context("git output not utf8")?;
    let p = s.trim();
    if p.is_empty() {
        bail!("git-path returned empty path for {rel}");
    }
    Ok(PathBuf::from(p))
}

pub(crate) fn write_agent_meta(agent_name: &str, meta: AgentMeta) -> Result<()> {
    let path = agent_meta_path(agent_name)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&meta)? + "\n";
    std::fs::write(&path, text).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

pub(crate) fn remove_agent_meta(agent_name: &str) -> Result<()> {
    let path = agent_meta_path(agent_name)?;
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    Ok(())
}
