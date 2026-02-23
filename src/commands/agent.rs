use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};

use crate::cli::{NewArgs as AgentNewArgs, RmArgs as AgentRmArgs};
use crate::exec;
use crate::git;
use crate::meta::{self, AgentMeta};
use crate::vscode;

use pc_cli::agent_name::{derive_agent_name_from_branch, is_valid_agent_name};

pub(crate) fn cmd_new(args: AgentNewArgs) -> Result<()> {
    exec::ensure_in_path("git")?;

    if !git::has_commit()? {
        bail!(
            "This git repository has no commits yet (unborn HEAD). \
Create an initial commit, then re-run `pc new ...`."
        );
    }

    let base_ref = match resolve_base_ref(&args)? {
        Some(v) => v,
        None => {
            println!("Cancelled.");
            return Ok(());
        }
    };

    let branch_name = match args.branch_name.clone() {
        Some(v) => v,
        None => {
            if args.base.is_some() || args.select_base {
                prompt_new_branch_name(&base_ref)?
            } else {
                match select_target_branch_tui()? {
                    Some(v) => v,
                    None => {
                        println!("Cancelled.");
                        return Ok(());
                    }
                }
            }
        }
    };

    let repo_root = git::repo_root()?;
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Failed to get repo name from path: {}", repo_root.display()))?
        .to_string();

    let worktree_base_dir = resolve_worktree_base_dir(&repo_root, &repo_name, args.base_dir)?;
    std::fs::create_dir_all(&worktree_base_dir)
        .with_context(|| format!("Failed to create base dir: {}", worktree_base_dir.display()))?;

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

    if let Some(existing) = git::worktree_path_for_branch(&branch_name)? {
        eprintln!(
            "Warning: worktree for branch already exists. Opening: {}",
            existing.display()
        );
        return reopen_existing_worktree(&branch_name, &agent_name, &existing, args.no_open);
    }

    let worktree_dir_raw = worktree_base_dir.join(&agent_name);
    if worktree_dir_raw.exists() {
        if let Some(entry) = git::worktree_entry_for_path(&worktree_dir_raw)? {
            if let Some(existing_ref) = entry.branch.as_deref() {
                let wanted_ref = format!("refs/heads/{branch_name}");
                if existing_ref != wanted_ref {
                    bail!(
                        "Worktree path already exists for a different branch: {} (existing: {})",
                        worktree_dir_raw.display(),
                        existing_ref
                            .strip_prefix("refs/heads/")
                            .unwrap_or(existing_ref)
                    );
                }
            }
        }
        eprintln!(
            "Warning: worktree path already exists. Opening: {}",
            worktree_dir_raw.display()
        );
        return reopen_existing_worktree(
            &branch_name,
            &agent_name,
            &worktree_dir_raw,
            args.no_open,
        );
    }

    if let Some(existing) = git::worktree_path_for_basename(&agent_name)? {
        if let Some(entry) = git::worktree_entry_for_path(&existing)? {
            if let Some(existing_ref) = entry.branch.as_deref() {
                let wanted_ref = format!("refs/heads/{branch_name}");
                if existing_ref != wanted_ref {
                    bail!(
                        "A worktree directory with the same name already exists for a different branch: {} (existing: {})",
                        existing.display(),
                        existing_ref.strip_prefix("refs/heads/").unwrap_or(existing_ref)
                    );
                }
            }
        }
        eprintln!(
            "Warning: worktree directory name already exists. Opening: {}",
            existing.display()
        );
        return reopen_existing_worktree(&branch_name, &agent_name, &existing, args.no_open);
    }

    git::ensure_ref_exists(&base_ref)?;

    let branch_exists = git::branch_exists_local(&branch_name)?;
    if !branch_exists {
        if exec::can_prompt() {
            eprintln!("Warning: branch does not exist: {branch_name}");
            let ok = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!("Create new branch {branch_name} from {base_ref}?"))
                .default(true)
                .interact()
                .context("Prompt failed")?;
            if !ok {
                println!("Cancelled. Branch not created: {branch_name}");
                return Ok(());
            }
        } else {
            eprintln!(
                "Warning: branch does not exist: {branch_name}. Creating it from {base_ref}."
            );
        }
    }

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

fn resolve_base_ref(args: &AgentNewArgs) -> Result<Option<String>> {
    if args.select_base && args.base.is_some() {
        bail!("Use either --base or --select-base, not both.");
    }

    if args.select_base {
        return select_base_branch_tui();
    }

    match args.base.clone() {
        Some(v) if v == "__tui__" => select_base_branch_tui(),
        Some(v) => Ok(Some(v)),
        None => Ok(Some("HEAD".to_string())),
    }
}

fn prompt_new_branch_name(base_ref: &str) -> Result<String> {
    if !dialoguer::console::Term::stdout().is_term() {
        bail!("No branch specified and no TTY available. Pass a branch name: `pc new <branch>`.");
    }

    let branch = Input::<String>::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("New branch name (base: {base_ref})"))
        .validate_with(|s: &String| {
            if s.trim().is_empty() {
                return Err("Branch name cannot be empty".to_string());
            }
            Ok(())
        })
        .interact_text()
        .context("Prompt failed")?;

    Ok(branch.trim().to_string())
}

fn reopen_existing_worktree(
    branch_name: &str,
    agent_name: &str,
    worktree_dir: &Path,
    no_open: bool,
) -> Result<()> {
    let worktree_dir =
        std::fs::canonicalize(worktree_dir).unwrap_or_else(|_| worktree_dir.to_path_buf());
    if agent_name != branch_name {
        println!("Agent:    {agent_name}");
    }
    println!("Worktree: {}", worktree_dir.display());
    println!("Branch:   {branch_name}");

    if !no_open && exec::is_in_path("code") {
        if let Err(e) = vscode::open_vscode_local(&worktree_dir) {
            eprintln!("Warning: failed to open VS Code: {e:#}");
        }
    }
    Ok(())
}

pub(crate) fn cmd_rm(args: AgentRmArgs) -> Result<()> {
    exec::ensure_in_path("git")?;

    let AgentRmArgs {
        branch_name: arg_branch_name,
        agent_name: arg_agent_name,
        base_dir,
        force,
    } = args;

    let repo_root = git::repo_root()?;
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Failed to get repo name from path: {}", repo_root.display()))?
        .to_string();

    let worktree_base_dir = resolve_worktree_base_dir(&repo_root, &repo_name, base_dir)?;

    if arg_branch_name.is_none() && arg_agent_name.is_some() {
        bail!("--agent-name requires an explicit branch name (or select a worktree and omit --agent-name).");
    }

    let (branch_name, agent_name, worktree_dir_raw, should_remove_meta) = match arg_branch_name {
        Some(branch_name) => {
            git::ensure_branch_name_valid(&branch_name)?;

            let agent_name = match arg_agent_name {
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

            (Some(branch_name), agent_name, worktree_dir, true)
        }
        None => {
            let selected = select_worktree_to_remove_tui(&repo_root, &worktree_base_dir)?;
            let Some(selected) = selected else {
                println!("Cancelled.");
                return Ok(());
            };
            (
                selected.branch_name,
                selected.agent_name,
                selected.path,
                selected.should_remove_meta,
            )
        }
    };

    let worktree_dir = std::fs::canonicalize(&worktree_dir_raw)
        .with_context(|| format!("Failed to resolve {}", worktree_dir_raw.display()))?;

    if exec::can_prompt() {
        let ok = confirm_double_rm(&worktree_dir, branch_name.as_deref(), &agent_name)?;
        if !ok {
            println!(
                "Cancelled. Worktree not removed: {}",
                worktree_dir.display()
            );
            return Ok(());
        }
    }

    // Best-effort: ignore typical generated dirs so `git worktree remove` doesn't
    // require `--force` after normal local development (e.g. uv creates .venv).
    git::ensure_exclude(&worktree_dir, ".venv/")?;
    git::ensure_exclude(&worktree_dir, "node_modules/")?;
    git::ensure_exclude(&worktree_dir, "target/")?;
    git::ensure_exclude(&worktree_dir, ".pytest_cache/")?;
    git::ensure_exclude(&worktree_dir, ".ruff_cache/")?;

    let removed = git::worktree_remove(&worktree_dir, force)?;
    if !removed {
        println!(
            "Cancelled. Worktree not removed: {}",
            worktree_dir.display()
        );
        return Ok(());
    }

    if should_remove_meta {
        meta::remove_agent_meta(&agent_name)?;
    } else {
        eprintln!(
            "Warning: selected worktree is outside the configured base dir; skipping metadata removal for agent {agent_name}"
        );
    }

    if let Some(branch_name) = branch_name.as_deref() {
        println!("Removed worktree for {branch_name}");
    } else {
        println!("Removed worktree {}", worktree_dir.display());
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct SelectedWorktree {
    path: PathBuf,
    branch_name: Option<String>,
    agent_name: String,
    should_remove_meta: bool,
}

fn select_worktree_to_remove_tui(
    repo_root: &Path,
    worktree_base_dir: &Path,
) -> Result<Option<SelectedWorktree>> {
    if !dialoguer::console::Term::stdout().is_term() {
        bail!("No worktree specified and no TTY available. Pass a branch name: `pc rm <branch>`.");
    }

    let repo_root = std::fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf());
    let base = std::fs::canonicalize(worktree_base_dir)
        .unwrap_or_else(|_| worktree_base_dir.to_path_buf());

    let worktrees = git::worktrees()?;
    let mut candidates: Vec<git::WorktreeEntry> = worktrees
        .into_iter()
        .filter(|e| {
            let p = std::fs::canonicalize(&e.path).unwrap_or_else(|_| e.path.clone());
            p != repo_root && p.starts_with(&base)
        })
        .collect();

    if candidates.is_empty() {
        let worktrees = git::worktrees()?;
        candidates = worktrees
            .into_iter()
            .filter(|e| {
                let p = std::fs::canonicalize(&e.path).unwrap_or_else(|_| e.path.clone());
                p != repo_root
            })
            .collect();
    }

    if candidates.is_empty() {
        bail!("No removable worktrees found in this repository");
    }

    candidates.sort_by(|a, b| a.path.cmp(&b.path));

    let items: Vec<String> = candidates
        .iter()
        .map(|e| {
            let branch = e
                .branch
                .as_deref()
                .and_then(|s| s.strip_prefix("refs/heads/"))
                .unwrap_or("(detached)");
            format!("{branch}  —  {}", e.path.display())
        })
        .collect();

    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt("Select worktree to remove")
        .items(&items)
        .default(0)
        .interact_opt()
        .context("TUI selection failed")?;

    let Some(idx) = selection else {
        return Ok(None);
    };

    let chosen = candidates[idx].clone();
    let path = chosen.path;

    let agent_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Failed to derive agent name from path: {}", path.display()))?
        .to_string();

    let branch_name = chosen
        .branch
        .as_deref()
        .and_then(|s| s.strip_prefix("refs/heads/"))
        .map(|s| s.to_string());

    let should_remove_meta = {
        let resolved_path = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let expected = base.join(&agent_name);
        resolved_path == expected
    };

    Ok(Some(SelectedWorktree {
        path,
        branch_name,
        agent_name,
        should_remove_meta,
    }))
}

fn confirm_double_rm(worktree_dir: &Path, branch_name: Option<&str>, agent_name: &str) -> Result<bool> {
    let label = branch_name.unwrap_or(agent_name);
    let mut prompt = format!("Remove worktree: {}", worktree_dir.display());
    if let Some(b) = branch_name {
        prompt.push_str(&format!(" (branch: {b})"));
    }

    let ok = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()
        .context("Prompt failed")?;
    if !ok {
        return Ok(false);
    }

    let typed = Input::<String>::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Type '{label}' to confirm"))
        .default("".to_string())
        .interact_text()
        .context("Prompt failed")?;

    Ok(typed.trim() == label)
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

fn select_base_branch_tui() -> Result<Option<String>> {
    if !dialoguer::console::Term::stdout().is_term() {
        bail!("Interactive base selection requires a TTY");
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
        .interact_opt()
        .context("TUI selection failed")?;
    Ok(selection.map(|idx| branches[idx].name.clone()))
}

fn select_target_branch_tui() -> Result<Option<String>> {
    if !dialoguer::console::Term::stdout().is_term() {
        bail!("No branch specified and no TTY available. Pass a branch name: `pc new <branch>`.");
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
        .with_prompt("Select branch to open as worktree")
        .items(&items)
        .default(0)
        .interact_opt()
        .context("TUI selection failed")?;
    Ok(selection.map(|idx| branches[idx].name.clone()))
}
