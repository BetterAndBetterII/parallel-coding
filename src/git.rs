use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::exec;

pub(crate) fn repo_root() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to run git rev-parse")?;
    if !output.status.success() {
        bail!("Not a git repository (git rev-parse --show-toplevel failed)");
    }
    let s = String::from_utf8(output.stdout).context("git output not utf8")?;
    let p = s.trim();
    if p.is_empty() {
        bail!("git repo root is empty");
    }
    Ok(PathBuf::from(p))
}

pub(crate) fn has_commit() -> Result<bool> {
    let status = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "HEAD"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to run git rev-parse --verify HEAD")?;
    Ok(status.success())
}

pub(crate) fn ensure_ref_exists(name: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to run git rev-parse --verify")?;
    if status.success() {
        Ok(())
    } else {
        bail!("Base ref not found: {name}");
    }
}

pub(crate) fn ensure_branch_name_valid(name: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["check-ref-format", "--branch", name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to run git check-ref-format --branch")?;
    if status.success() {
        Ok(())
    } else {
        bail!("Invalid branch name: {name}");
    }
}

pub(crate) fn worktree_add(worktree_dir: &Path, branch_name: &str, base_ref: &str) -> Result<bool> {
    let ref_name = format!("refs/heads/{branch_name}");
    let branch_exists = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &ref_name])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let mut cmd = Command::new("git");
    if branch_exists {
        cmd.args(["worktree", "add"])
            .arg(worktree_dir)
            .arg(branch_name);
    } else {
        cmd.args(["worktree", "add", "-b"])
            .arg(branch_name)
            .arg(worktree_dir)
            .arg(base_ref);
    }
    exec::run_ok(cmd).context("git worktree add failed")?;
    Ok(!branch_exists)
}

pub(crate) fn worktree_remove(path: &Path, force: bool) -> Result<bool> {
    if force {
        let mut cmd = Command::new("git");
        cmd.args(["worktree", "remove", "--force"]).arg(path);
        exec::run_ok(cmd).context("git worktree remove failed")?;
        return Ok(true);
    }
    worktree_remove_interactive(path)
}

fn worktree_remove_interactive(path: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["worktree", "remove"])
        .arg(path)
        .output()
        .context("Failed to run git worktree remove")?;
    if output.status.success() {
        return Ok(true);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr_trimmed = stderr.trim();

    let suggests_force = stderr_trimmed.contains("use --force");
    if suggests_force && exec::can_prompt() {
        println!("{stderr_trimmed}");
        if let Ok(p) = status_porcelain(path) {
            if !p.trim().is_empty() {
                println!("Worktree has local changes/untracked files:");
                println!("{p}");
            }
        }
        let ok = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "git worktree remove failed ({}). Retry with --force?",
                path.display()
            ))
            .default(false)
            .interact()
            .context("Prompt failed")?;
        if !ok {
            return Ok(false);
        }
        let status = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(path)
            .status()
            .context("Failed to run git worktree remove --force")?;
        if status.success() {
            return Ok(true);
        }
        bail!("git worktree remove --force failed with status: {status}");
    }

    if stderr_trimmed.is_empty() {
        bail!("git worktree remove failed with status: {}", output.status);
    }
    bail!("git worktree remove failed: {stderr_trimmed}");
}

fn status_porcelain(worktree_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .current_dir(worktree_dir)
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .context("Failed to run git status")?;
    if !output.status.success() {
        bail!("git status failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub(crate) fn branch_delete_force(repo_root: &Path, branch_name: &str) -> Result<()> {
    let ref_name = format!("refs/heads/{branch_name}");
    let exists = Command::new("git")
        .current_dir(repo_root)
        .args(["show-ref", "--verify", "--quiet", &ref_name])
        .status()
        .context("Failed to run git show-ref --verify")?;
    if !exists.success() {
        return Ok(());
    }

    let status = Command::new("git")
        .current_dir(repo_root)
        .args(["branch", "-D", branch_name])
        .status()
        .context("Failed to run git branch -D")?;
    if status.success() {
        Ok(())
    } else {
        bail!("git branch -D {branch_name} failed with status: {status}");
    }
}

pub(crate) fn worktree_path_for_branch(branch_name: &str) -> Result<Option<PathBuf>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .context("Failed to run git worktree list")?;
    if !output.status.success() {
        bail!("git worktree list failed");
    }
    let text = String::from_utf8(output.stdout).context("git output not utf8")?;

    let wanted = format!("refs/heads/{branch_name}");
    let mut current_path: Option<PathBuf> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            current_path = Some(PathBuf::from(rest.trim()));
            continue;
        }
        if let Some(rest) = line.strip_prefix("branch ") {
            if rest.trim() == wanted {
                return Ok(current_path.clone());
            }
        }
    }
    Ok(None)
}

pub(crate) fn worktree_path_for_basename(name: &str) -> Result<Option<PathBuf>> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .output()
        .context("Failed to run git worktree list")?;
    if !output.status.success() {
        bail!("git worktree list failed");
    }
    let text = String::from_utf8(output.stdout).context("git output not utf8")?;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            let p = PathBuf::from(rest.trim());
            if p.file_name().and_then(|s| s.to_str()) == Some(name) {
                return Ok(Some(p));
            }
        }
    }
    Ok(None)
}

pub(crate) struct BranchInfo {
    pub(crate) name: String,
    pub(crate) committer_date: String,
}

pub(crate) fn local_branches_by_recent() -> Result<Vec<BranchInfo>> {
    let output = Command::new("git")
        .args([
            "for-each-ref",
            "--sort=-committerdate",
            "--format=%(refname:short)\t%(committerdate:iso8601)",
            "refs/heads/",
        ])
        .output()
        .context("Failed to run git for-each-ref")?;
    if !output.status.success() {
        bail!("git for-each-ref failed");
    }
    let text = String::from_utf8(output.stdout).context("git output not utf8")?;
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, date) = line.split_once('\t').unwrap_or((line, ""));
        out.push(BranchInfo {
            name: name.to_string(),
            committer_date: date.to_string(),
        });
    }
    Ok(out)
}

pub(crate) fn ensure_exclude(worktree_dir: &Path, pattern: &str) -> Result<()> {
    let output = Command::new("git")
        .current_dir(worktree_dir)
        .args(["rev-parse", "--git-path", "info/exclude"])
        .output()
        .context("Failed to run git rev-parse --git-path info/exclude")?;
    if !output.status.success() {
        bail!("git rev-parse --git-path info/exclude failed");
    }
    let path = String::from_utf8(output.stdout).context("git output not utf8")?;
    let exclude_path = PathBuf::from(path.trim());
    let mut existing = String::new();
    if exclude_path.exists() {
        existing = std::fs::read_to_string(&exclude_path)
            .with_context(|| format!("Failed to read {}", exclude_path.display()))?;
        if existing.lines().any(|l| l.trim() == pattern) {
            return Ok(());
        }
    }
    if !existing.ends_with('\n') && !existing.is_empty() {
        existing.push('\n');
    }
    existing.push_str(pattern);
    existing.push('\n');
    std::fs::write(&exclude_path, existing)
        .with_context(|| format!("Failed to write {}", exclude_path.display()))?;
    Ok(())
}
