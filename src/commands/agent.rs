use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use dialoguer::{theme::ColorfulTheme, Select};

use crate::cli::{AgentNewArgs, AgentRmArgs};
use crate::exec;
use crate::git;
use crate::meta::{self, AgentMeta};
use crate::vscode;

use pc_cli::agent_name::{derive_agent_name_from_branch, is_valid_agent_name};

pub(crate) fn cmd_agent_new(args: AgentNewArgs) -> Result<()> {
    exec::ensure_in_path("git")?;

    if !git::has_commit()? {
        bail!(
            "This git repository has no commits yet (unborn HEAD). \
Create an initial commit, then re-run `pc agent new ...`."
        );
    }

    let repo_root = git::repo_root()?;
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Failed to get repo name from path: {}", repo_root.display()))?
        .to_string();

    let worktree_base_dir = resolve_worktree_base_dir(&repo_root, &repo_name, args.base_dir)?;
    std::fs::create_dir_all(&worktree_base_dir)
        .with_context(|| format!("Failed to create base dir: {}", worktree_base_dir.display()))?;

    let branch_name = args.branch_name.clone();
    git::ensure_branch_name_valid(&branch_name)?;

    let agent_name = match args.agent_name {
        Some(v) => {
            if !is_valid_agent_name(&v) {
                bail!("agent-name must match: [A-Za-z0-9._-]+ (and cannot be '.' or '..')");
            }
            v
        }
        None => derive_agent_name_from_branch(&branch_name)?,
    };

    let worktree_dir_raw = worktree_base_dir.join(&agent_name);
    if worktree_dir_raw.exists() {
        bail!(
            "Worktree path already exists: {}",
            worktree_dir_raw.display()
        );
    }

    if let Some(existing) = git::worktree_path_for_basename(&agent_name)? {
        bail!(
            "A worktree directory with the same name already exists: {}",
            existing.display()
        );
    }
    if let Some(existing) = git::worktree_path_for_branch(&branch_name)? {
        bail!(
            "Worktree for branch {} already exists at: {}",
            branch_name,
            existing.display()
        );
    }

    if args.select_base && args.base.is_some() {
        bail!("Use either --base or --select-base, not both.");
    }

    let base_ref = if args.select_base {
        select_base_branch_tui()?
    } else {
        args.base.clone().unwrap_or_else(|| "HEAD".to_string())
    };
    git::ensure_ref_exists(&base_ref)?;

    let created_branch = git::worktree_add(&worktree_dir_raw, &branch_name, &base_ref)?;

    let worktree_dir = match std::fs::canonicalize(&worktree_dir_raw) {
        Ok(p) => p,
        Err(e) => {
            rollback_failed_agent_new(
                &repo_root,
                &agent_name,
                &worktree_dir_raw,
                &branch_name,
                created_branch,
            )?;
            return Err(anyhow::Error::new(e).context(format!(
                "Failed to resolve worktree dir: {}",
                worktree_dir_raw.display()
            )));
        }
    };

    if agent_name != branch_name {
        println!("Agent:    {agent_name}");
    }
    println!("Worktree: {}", worktree_dir.display());
    println!("Branch:   {branch_name}");

    if let Err(e) = meta::write_agent_meta(
        &agent_name,
        AgentMeta {
            branch_name: Some(branch_name.clone()),
        },
    ) {
        rollback_failed_agent_new(
            &repo_root,
            &agent_name,
            &worktree_dir,
            &branch_name,
            created_branch,
        )?;
        return Err(e);
    }

    if !args.no_open && exec::is_in_path("code") {
        if let Err(e) = vscode::open_vscode_local(&worktree_dir) {
            eprintln!("Warning: failed to open VS Code: {e:#}");
        }
    }

    Ok(())
}

pub(crate) fn cmd_agent_rm(args: AgentRmArgs) -> Result<()> {
    exec::ensure_in_path("git")?;

    let repo_root = git::repo_root()?;
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Failed to get repo name from path: {}", repo_root.display()))?
        .to_string();

    let worktree_base_dir = resolve_worktree_base_dir(&repo_root, &repo_name, args.base_dir)?;

    let branch_name = args.branch_name.clone();
    git::ensure_branch_name_valid(&branch_name)?;

    let agent_name = match args.agent_name {
        Some(v) => {
            if !is_valid_agent_name(&v) {
                bail!("agent-name must match: [A-Za-z0-9._-]+ (and cannot be '.' or '..')");
            }
            v
        }
        None => derive_agent_name_from_branch(&branch_name)?,
    };

    let expected_dir = worktree_base_dir.join(&agent_name);
    let worktree_dir = if expected_dir.exists() {
        expected_dir
    } else if let Some(p) = git::worktree_path_for_branch(&branch_name)? {
        p
    } else {
        bail!(
            "Agent worktree not found. Expected path: {} (branch: {})",
            expected_dir.display(),
            branch_name
        );
    };

    let worktree_dir = std::fs::canonicalize(&worktree_dir)
        .with_context(|| format!("Failed to resolve {}", worktree_dir.display()))?;

    // Best-effort: ignore typical generated dirs so `git worktree remove` doesn't
    // require `--force` after normal local development (e.g. uv creates .venv).
    git::ensure_exclude(&worktree_dir, ".venv/")?;
    git::ensure_exclude(&worktree_dir, "node_modules/")?;
    git::ensure_exclude(&worktree_dir, "target/")?;
    git::ensure_exclude(&worktree_dir, ".pytest_cache/")?;
    git::ensure_exclude(&worktree_dir, ".ruff_cache/")?;

    let removed = git::worktree_remove(&worktree_dir, args.force)?;
    if !removed {
        println!(
            "Cancelled. Worktree not removed: {}",
            worktree_dir.display()
        );
        return Ok(());
    }

    meta::remove_agent_meta(&agent_name)?;

    println!("Removed agent {agent_name}");
    Ok(())
}

fn resolve_worktree_base_dir(
    repo_root: &Path,
    repo_name: &str,
    arg_base_dir: Option<PathBuf>,
) -> Result<PathBuf> {
    Ok(if let Some(d) = arg_base_dir {
        d
    } else if let Some(env) = std::env::var_os("AGENT_WORKTREE_BASE_DIR") {
        PathBuf::from(env)
    } else {
        let parent = repo_root
            .parent()
            .ok_or_else(|| anyhow!("Repo root has no parent: {}", repo_root.display()))?;
        parent.join(format!("{repo_name}-agents"))
    })
}

fn rollback_failed_agent_new(
    repo_root: &Path,
    agent_name: &str,
    worktree_dir: &Path,
    branch_name: &str,
    created_branch: bool,
) -> Result<()> {
    if let Err(e) = git::worktree_remove(worktree_dir, true) {
        eprintln!(
            "Warning: git worktree remove --force failed during rollback for {}: {e:#}",
            worktree_dir.display()
        );
    }
    if created_branch {
        if let Err(e) = git::branch_delete_force(repo_root, branch_name) {
            eprintln!(
                "Warning: git branch -D failed during rollback for {}: {e:#}",
                branch_name
            );
        }
    }
    if let Err(e) = meta::remove_agent_meta(agent_name) {
        eprintln!(
            "Warning: failed to remove agent metadata during rollback for {}: {e:#}",
            agent_name
        );
    }
    Ok(())
}

fn select_base_branch_tui() -> Result<String> {
    if !dialoguer::console::Term::stdout().is_term() {
        bail!("--select-base requires a TTY");
    }

    let branches = git::local_branches_by_recent()?;
    if branches.is_empty() {
        bail!("No local branches found");
    }

    let items: Vec<String> = branches
        .iter()
        .map(|b| format!("{}  ({})", b.name, b.committer_date))
        .collect();
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select base branch")
        .items(&items)
        .default(0)
        .interact()
        .context("TUI selection failed")?;
    Ok(branches[selection].name.clone())
}
