use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, Confirm, Select};

mod templates;

#[derive(Parser, Debug)]
#[command(
    name = "pc",
    version,
    about = "Parallel containers controller (git worktree + devcontainer)"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize .devcontainer in a directory using an embedded preset
    Init(InitArgs),
    /// Bring up a Dev Container from any directory
    Up(UpArgs),
    /// Start the optional desktop (webtop) sidecar for a directory
    DesktopOn(DesktopOnArgs),
    /// Manage devcontainer templates under $HOME/.pc/templates
    Templates(TemplatesArgs),
    /// Git worktree based agent workflows
    Agent(AgentArgs),
}

#[derive(Args, Debug)]
struct InitArgs {
    /// Target directory
    dir: PathBuf,
    /// Preset template name
    #[arg(long, default_value = "python-uv")]
    preset: String,
    /// Overwrite existing files
    #[arg(long)]
    force: bool,
}

#[derive(Args, Debug)]
struct UpArgs {
    /// Target directory
    dir: PathBuf,
    /// Initialize if missing .devcontainer
    #[arg(long)]
    init: bool,
    /// Preset template name (used with --init)
    #[arg(long, default_value = "python-uv")]
    preset: String,
    /// Also bring up desktop sidecar
    #[arg(long)]
    desktop: bool,
    /// Overwrite existing .devcontainer/.env
    #[arg(long)]
    force_env: bool,
}

#[derive(Args, Debug)]
struct DesktopOnArgs {
    /// Target directory
    dir: PathBuf,
}

#[derive(Args, Debug)]
struct TemplatesArgs {
    #[command(subcommand)]
    command: TemplatesCommands,
}

#[derive(Subcommand, Debug)]
enum TemplatesCommands {
    /// Install embedded presets into $HOME/.pc/templates for customization
    Init(TemplatesInitArgs),
}

#[derive(Args, Debug)]
struct TemplatesInitArgs {
    /// Overwrite existing files
    #[arg(long)]
    force: bool,
}

#[derive(Args, Debug)]
struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommands,
}

#[derive(Subcommand, Debug)]
enum AgentCommands {
    /// Create git worktree + branch and (optionally) boot devcontainer
    New(AgentNewArgs),
    /// Start the optional desktop (webtop) sidecar for a given worktree path
    DesktopOn(AgentDesktopOnArgs),
    /// Remove an agent: docker compose down + git worktree remove (+ branch delete)
    Rm(AgentRmArgs),
}

#[derive(Args, Debug)]
struct AgentNewArgs {
    /// Agent name (used in branch name and compose project name)
    agent_name: String,
    /// Base branch/ref for the new worktree branch (default: current HEAD)
    #[arg(long)]
    base: Option<String>,
    /// Select base branch with an interactive TUI (sorted by recent updates)
    #[arg(long)]
    select_base: bool,
    /// Devcontainer template preset to use when the worktree has no .devcontainer
    #[arg(long, default_value = "python-uv")]
    preset: String,
    /// Base directory to place worktrees
    #[arg(long)]
    base_dir: Option<PathBuf>,
    /// Do not run devcontainer up
    #[arg(long)]
    no_up: bool,
    /// Also start desktop sidecar
    #[arg(long)]
    desktop: bool,
    /// Overwrite existing .devcontainer/.env
    #[arg(long)]
    force_env: bool,
    /// Do not open VS Code in a new window
    #[arg(long)]
    no_open: bool,
}

#[derive(Args, Debug)]
struct AgentDesktopOnArgs {
    /// Worktree path
    worktree_path: PathBuf,
}

#[derive(Args, Debug)]
struct AgentRmArgs {
    /// Agent name (used in branch name and default worktree path)
    agent_name: String,
    /// Base directory to place worktrees (for locating existing worktree dir)
    #[arg(long)]
    base_dir: Option<PathBuf>,
    /// Keep the agent branch (do not `git branch -D`)
    #[arg(long)]
    keep_branch: bool,
    /// Force removal (passes --force to git worktree remove)
    #[arg(long)]
    force: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init(args) => cmd_init(args),
        Commands::Up(args) => cmd_up(args),
        Commands::DesktopOn(args) => cmd_desktop_on(args.dir),
        Commands::Templates(args) => cmd_templates(args),
        Commands::Agent(args) => match args.command {
            AgentCommands::New(a) => cmd_agent_new(a),
            AgentCommands::DesktopOn(a) => cmd_desktop_on(a.worktree_path),
            AgentCommands::Rm(a) => cmd_agent_rm(a),
        },
    }
}

fn cmd_templates(args: TemplatesArgs) -> Result<()> {
    match args.command {
        TemplatesCommands::Init(a) => cmd_templates_init(a),
    }
}

fn cmd_templates_init(args: TemplatesInitArgs) -> Result<()> {
    for preset in templates::embedded_presets() {
        let dir = match templates::install_embedded_preset(preset, args.force) {
            Ok(d) => d,
            Err(e)
                if !args.force
                    && can_prompt()
                    && e.downcast_ref::<templates::ForceRequired>().is_some() =>
            {
                let ok = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(format!(
                        "Template files already exist for preset {preset}. Overwrite? (equivalent to --force)"
                    ))
                    .default(false)
                    .interact()
                    .context("Prompt failed")?;
                if !ok {
                    println!("Skipped preset {preset} (left existing files).");
                    continue;
                }
                templates::install_embedded_preset(preset, true)?
            }
            Err(e) => return Err(e),
        };
        println!("Installed preset {preset} into {}", dir.display());
    }
    println!("Edit templates under $HOME/.pc/templates/<preset>/ to customize.");
    println!("Tip: set PC_HOME=/some/dir to override $HOME/.pc.");
    Ok(())
}

fn cmd_init(args: InitArgs) -> Result<()> {
    let dir = require_existing_dir(&args.dir)?;
    copy_preset(&args.preset, &dir, args.force)?;
    write_env_for_dir(&dir, false)?;
    println!(
        "Initialized: {} (preset: {})",
        dir.join(".devcontainer").display(),
        args.preset
    );
    Ok(())
}

fn cmd_up(args: UpArgs) -> Result<()> {
    let dir = require_existing_dir(&args.dir)?;

    let devcontainer_json = dir.join(".devcontainer").join("devcontainer.json");
    if !devcontainer_json.exists() {
        if args.init {
            copy_preset(&args.preset, &dir, false)?;
        } else {
            bail!(
                "Missing {} (use: pc up --init ...)",
                devcontainer_json.display()
            );
        }
    }

    write_env_for_dir(&dir, args.force_env)?;
    ensure_in_path("devcontainer")?;

    if args.desktop {
        devcontainer_up(&dir, Some(("COMPOSE_PROFILES", "desktop")))?;
    } else {
        devcontainer_up(&dir, None)?;
    }
    Ok(())
}

fn cmd_desktop_on(dir: PathBuf) -> Result<()> {
    let dir = require_existing_dir(&dir)?;
    let compose_path = dir.join(".devcontainer").join("compose.yaml");
    if !compose_path.exists() {
        bail!(
            "Not a devcontainer directory (missing {}): {}",
            ".devcontainer/compose.yaml",
            dir.display()
        );
    }
    ensure_in_path("devcontainer")?;
    write_env_for_dir(&dir, false)?;
    devcontainer_up(&dir, Some(("COMPOSE_PROFILES", "desktop")))?;

    if is_in_path("docker") {
        if let Some(url) = try_get_desktop_url(&dir)? {
            println!("Desktop URL: {url}");
        } else {
            println!("Desktop started. To get the URL:");
            println!(
                "  (cd \"{}\" && docker compose -f compose.yaml port desktop 3000)",
                dir.join(".devcontainer").display()
            );
        }
    }
    Ok(())
}

fn cmd_agent_new(args: AgentNewArgs) -> Result<()> {
    if !is_valid_agent_name(&args.agent_name) {
        bail!("agent-name must match: [A-Za-z0-9._-]+");
    }

    ensure_in_path("git")?;

    if !git_has_commit()? {
        bail!(
            "This git repository has no commits yet (unborn HEAD). \
git worktree will create an orphan branch and the worktree will be empty, \
so devcontainer config like .devcontainer/devcontainer.json will be missing.\n\
Fix: create an initial commit, then re-run `pc agent new ...`."
        );
    }

    let repo_root = git_repo_root()?;
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Failed to get repo name from path: {}", repo_root.display()))?
        .to_string();

    let worktree_base_dir = if let Some(d) = args.base_dir {
        d
    } else if let Some(env) = std::env::var_os("AGENT_WORKTREE_BASE_DIR") {
        PathBuf::from(env)
    } else {
        let parent = repo_root
            .parent()
            .ok_or_else(|| anyhow!("Repo root has no parent: {}", repo_root.display()))?;
        parent.join(format!("{repo_name}-agents"))
    };

    std::fs::create_dir_all(&worktree_base_dir)
        .with_context(|| format!("Failed to create base dir: {}", worktree_base_dir.display()))?;

    let worktree_dir = worktree_base_dir.join(&args.agent_name);
    if worktree_dir.exists() {
        bail!("Worktree path already exists: {}", worktree_dir.display());
    }

    let branch_name = format!("agent/{}", args.agent_name);
    if let Some(existing) = git_worktree_path_for_basename(&args.agent_name)? {
        bail!(
            "A worktree directory with the same name already exists: {}",
            existing.display()
        );
    }
    if let Some(existing) = git_worktree_path_for_branch(&branch_name)? {
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

    ensure_git_ref_exists(&base_ref)?;
    git_worktree_add(&worktree_dir, &branch_name, &base_ref)?;
    let worktree_dir = std::fs::canonicalize(&worktree_dir)
        .with_context(|| format!("Failed to resolve worktree dir: {}", worktree_dir.display()))?;

    let compose_project = format!("agent_{}", sanitize_compose(&args.agent_name));
    let env_file = worktree_dir.join(".devcontainer").join(".env");
    std::fs::create_dir_all(worktree_dir.join(".devcontainer"))
        .context("Failed to create .devcontainer directory")?;

    if env_file.exists() && !args.force_env {
        if can_prompt() {
            let ok = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!(
                    "{} already exists. Overwrite it? (equivalent to --force-env)",
                    env_file.display()
                ))
                .default(false)
                .interact()
                .context("Prompt failed")?;
            if !ok {
                println!("Cancelled. Left existing {}", env_file.display());
                return Ok(());
            }
        } else {
            bail!(
                "{} already exists. Use --force-env to overwrite.",
                env_file.display()
            );
        }
    }

    let env_contents = format!(
        "# Auto-generated by pc\nCOMPOSE_PROJECT_NAME={}\nDEVCONTAINER_CACHE_PREFIX={}\n",
        compose_project,
        sanitize_compose(&repo_name)
    );
    std::fs::write(&env_file, env_contents)
        .with_context(|| format!("Failed to write {}", env_file.display()))?;
    ensure_git_exclude(&worktree_dir, ".devcontainer/.env")?;

    println!("Worktree: {}", worktree_dir.display());
    println!("Branch:   {branch_name}");
    println!("Compose:  {compose_project}");

    if args.no_up {
        return Ok(());
    }

    let devcontainer_json = worktree_dir.join(".devcontainer").join("devcontainer.json");
    if !devcontainer_json.exists() {
        println!(
            "Devcontainer config missing in worktree; initializing from preset: {}",
            args.preset
        );
        copy_preset(&args.preset, &worktree_dir, false)?;
        ensure_git_exclude(&worktree_dir, ".devcontainer/devcontainer.json")?;
        ensure_git_exclude(&worktree_dir, ".devcontainer/compose.yaml")?;
        ensure_git_exclude(&worktree_dir, ".devcontainer/Dockerfile")?;
    }

    ensure_in_path("devcontainer")?;
    devcontainer_up(&worktree_dir, None)?;
    if args.desktop {
        devcontainer_up(&worktree_dir, Some(("COMPOSE_PROFILES", "desktop")))?;
    }

    if !args.no_open && is_in_path("code") {
        let _ = Command::new("code")
            .arg("--new-window")
            .arg(&worktree_dir)
            .status();
    }

    Ok(())
}

fn cmd_agent_rm(args: AgentRmArgs) -> Result<()> {
    if !is_valid_agent_name(&args.agent_name) {
        bail!("agent-name must match: [A-Za-z0-9._-]+");
    }
    ensure_in_path("git")?;

    let repo_root = git_repo_root()?;
    let repo_name = repo_root
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow!("Failed to get repo name from path: {}", repo_root.display()))?
        .to_string();

    let worktree_base_dir = if let Some(d) = args.base_dir {
        d
    } else if let Some(env) = std::env::var_os("AGENT_WORKTREE_BASE_DIR") {
        PathBuf::from(env)
    } else {
        let parent = repo_root
            .parent()
            .ok_or_else(|| anyhow!("Repo root has no parent: {}", repo_root.display()))?;
        parent.join(format!("{repo_name}-agents"))
    };

    let expected_dir = worktree_base_dir.join(&args.agent_name);
    let branch_name = format!("agent/{}", args.agent_name);

    let worktree_dir = if expected_dir.exists() {
        expected_dir
    } else if let Some(p) = git_worktree_path_for_branch(&branch_name)? {
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

    docker_compose_down_if_present(&worktree_dir)?;
    let removed = git_worktree_remove(&worktree_dir, args.force)?;
    if !removed {
        println!(
            "Cancelled. Worktree not removed: {}",
            worktree_dir.display()
        );
        return Ok(());
    }

    if !args.keep_branch {
        if git_branch_exists(&branch_name)? {
            let status = Command::new("git")
                .args(["branch", "-D", &branch_name])
                .status()
                .context("Failed to run git branch -D")?;
            if !status.success() {
                bail!("git branch -D failed with status: {status}");
            }
        }
    }

    println!("Removed agent {}", args.agent_name);
    Ok(())
}

fn copy_preset(preset: &str, dir: &Path, force: bool) -> Result<()> {
    let files = templates::preset_files(preset)?;

    let devcontainer_dir = dir.join(".devcontainer");
    std::fs::create_dir_all(&devcontainer_dir)
        .with_context(|| format!("Failed to create {}", devcontainer_dir.display()))?;

    let needs_overwrite = files
        .iter()
        .any(|(name, _)| devcontainer_dir.join(name).exists());
    let overwrite_all = if force {
        true
    } else if needs_overwrite && can_prompt() {
        Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Some files already exist under {}. Overwrite them? (equivalent to --force)",
                devcontainer_dir.display()
            ))
            .default(false)
            .interact()
            .context("Prompt failed")?
    } else {
        false
    };

    for (name, contents) in files {
        let target = devcontainer_dir.join(&name);
        if target.exists() && !overwrite_all {
            continue;
        }
        std::fs::write(&target, contents).with_context(|| {
            format!(
                "Failed to write preset file {} to {}",
                preset,
                target.display()
            )
        })?;
    }
    Ok(())
}

fn write_env_for_dir(dir: &Path, force_env: bool) -> Result<()> {
    let devcontainer_dir = dir.join(".devcontainer");
    std::fs::create_dir_all(&devcontainer_dir)
        .with_context(|| format!("Failed to create {}", devcontainer_dir.display()))?;

    let env_file = devcontainer_dir.join(".env");
    if env_file.exists() && !force_env {
        return Ok(());
    }

    let abs = std::fs::canonicalize(dir)
        .with_context(|| format!("Failed to resolve directory: {}", dir.display()))?;
    let base = abs
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace");

    let project = format!("dc_{}_{}", sanitize_compose(base), short_hash(&abs));
    let contents = format!(
        "# Auto-generated by pc\nCOMPOSE_PROJECT_NAME={}\nDEVCONTAINER_CACHE_PREFIX=dc-cache\n",
        project
    );
    std::fs::write(&env_file, contents)
        .with_context(|| format!("Failed to write {}", env_file.display()))?;
    Ok(())
}

fn devcontainer_up(dir: &Path, env: Option<(&str, &str)>) -> Result<()> {
    let mut cmd = Command::new("devcontainer");
    cmd.arg("up").arg("--workspace-folder").arg(dir);
    if let Some((k, v)) = env {
        cmd.env(k, v);
    }
    run_ok(cmd).context("devcontainer up failed")?;
    Ok(())
}

fn ensure_in_path(bin: &str) -> Result<()> {
    if is_in_path(bin) {
        Ok(())
    } else {
        bail!("{bin} not found in PATH");
    }
}

fn is_in_path(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

fn require_existing_dir(dir: &Path) -> Result<PathBuf> {
    if !dir.exists() {
        bail!("Directory not found: {}", dir.display());
    }
    let meta =
        std::fs::metadata(dir).with_context(|| format!("Failed to stat {}", dir.display()))?;
    if !meta.is_dir() {
        bail!("Not a directory: {}", dir.display());
    }
    Ok(std::fs::canonicalize(dir)
        .with_context(|| format!("Failed to resolve {}", dir.display()))?)
}

fn run_ok(mut cmd: Command) -> Result<ExitStatus> {
    let status = cmd.status().context("Failed to spawn command")?;
    if status.success() {
        Ok(status)
    } else {
        bail!("Command failed with status: {status}");
    }
}

fn short_hash(path: &Path) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(path.to_string_lossy().as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex.chars().take(8).collect()
}

fn sanitize_compose(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "workspace".to_string()
    } else {
        out
    }
}

fn is_valid_agent_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

fn git_repo_root() -> Result<PathBuf> {
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

fn git_has_commit() -> Result<bool> {
    let status = Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "HEAD"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to run git rev-parse --verify HEAD")?;
    Ok(status.success())
}

fn git_branch_exists(branch_name: &str) -> Result<bool> {
    let ref_name = format!("refs/heads/{branch_name}");
    let status = Command::new("git")
        .args(["show-ref", "--verify", "--quiet", &ref_name])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to run git show-ref --verify")?;
    Ok(status.success())
}

fn ensure_git_ref_exists(name: &str) -> Result<()> {
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

fn git_worktree_add(worktree_dir: &Path, branch_name: &str, base_ref: &str) -> Result<()> {
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
    run_ok(cmd).context("git worktree add failed")?;
    Ok(())
}

fn git_worktree_remove(path: &Path, force: bool) -> Result<bool> {
    if !force {
        return git_worktree_remove_interactive(path);
    }

    let mut cmd = Command::new("git");
    cmd.args(["worktree", "remove"]);
    if force {
        cmd.arg("--force");
    }
    cmd.arg(path);
    run_ok(cmd).context("git worktree remove failed")?;
    Ok(true)
}

fn git_worktree_remove_interactive(path: &Path) -> Result<bool> {
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
    if suggests_force && can_prompt() {
        println!("{stderr_trimmed}");
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

fn git_worktree_path_for_branch(branch_name: &str) -> Result<Option<PathBuf>> {
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

fn git_worktree_path_for_basename(name: &str) -> Result<Option<PathBuf>> {
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

fn docker_compose_down_if_present(worktree_dir: &Path) -> Result<()> {
    if !is_in_path("docker") {
        return Ok(());
    }
    let dc_dir = worktree_dir.join(".devcontainer");
    let compose = dc_dir.join("compose.yaml");
    if !compose.exists() {
        return Ok(());
    }

    let env_file = dc_dir.join(".env");
    let mut cmd = Command::new("docker");
    cmd.current_dir(&dc_dir)
        .args(["compose", "-f", "compose.yaml"]);
    if env_file.exists() {
        cmd.args(["--env-file", ".env"]);
    }
    cmd.args(["down", "-v", "--remove-orphans"]);

    let status = cmd
        .status()
        .context("Failed to spawn docker compose down")?;
    if !status.success() {
        bail!("docker compose down failed with status: {status}");
    }
    Ok(())
}

fn select_base_branch_tui() -> Result<String> {
    if !dialoguer::console::Term::stdout().is_term() {
        bail!("--select-base requires a TTY");
    }

    let branches = git_local_branches_by_recent()?;
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

struct BranchInfo {
    name: String,
    committer_date: String,
}

fn git_local_branches_by_recent() -> Result<Vec<BranchInfo>> {
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

fn can_prompt() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

fn ensure_git_exclude(worktree_dir: &Path, pattern: &str) -> Result<()> {
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

fn try_get_desktop_url(dir: &Path) -> Result<Option<String>> {
    let dc_dir = dir.join(".devcontainer");
    let output = Command::new("docker")
        .current_dir(&dc_dir)
        .args(["compose", "-f", "compose.yaml", "port", "desktop", "3000"])
        .output()
        .context("Failed to run docker compose port")?;
    if !output.status.success() {
        return Ok(None);
    }
    let s = String::from_utf8_lossy(&output.stdout);
    let mapping = s.trim();
    if mapping.is_empty() {
        return Ok(None);
    }
    // docker compose port prints like "0.0.0.0:49153" or "127.0.0.1:49153"
    let (host, port) = mapping
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("Unexpected docker port output: {mapping:?}"))?;
    let host = if host == "0.0.0.0" { "127.0.0.1" } else { host };
    Ok(Some(format!("http://{host}:{port}/")))
}
