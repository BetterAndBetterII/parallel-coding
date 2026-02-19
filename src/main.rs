use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use serde::{Deserialize, Serialize};

mod templates;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AgentMeta {
    preset: String,
    compose_project: String,
    cache_prefix: String,
}

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
    /// If missing devcontainer config, write preset files into the workspace
    #[arg(long)]
    init: bool,
    /// Preset template name (used for stealth mode and/or with --init)
    #[arg(long, default_value = "python-uv")]
    preset: String,
    /// Also bring up desktop sidecar
    #[arg(long)]
    desktop: bool,
    /// Overwrite generated runtime preset files (stealth mode)
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
    /// Compose a custom template from selected stacks (writes to $HOME/.pc/templates/<name>/)
    Compose(TemplatesComposeArgs),
}

#[derive(Args, Debug)]
struct TemplatesInitArgs {
    /// Overwrite existing files
    #[arg(long)]
    force: bool,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum StackComponent {
    Python,
    Uv,
    Go,
    Node,
    Pnpm,
    Desktop,
}

#[derive(Args, Debug)]
struct TemplatesComposeArgs {
    /// Template name (directory under $HOME/.pc/templates/)
    name: String,
    /// Components to include (can be repeated)
    #[arg(long = "with", value_enum)]
    with_components: Vec<StackComponent>,
    /// Select components interactively (TUI)
    #[arg(long)]
    interactive: bool,
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
    /// Remove an agent: docker compose down + git worktree remove
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
    /// Overwrite generated runtime preset files (stealth mode)
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
        TemplatesCommands::Compose(a) => cmd_templates_compose(a),
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

fn cmd_templates_compose(args: TemplatesComposeArgs) -> Result<()> {
    let mut spec = templates::StackSpec::default();

    if args.interactive {
        if !can_prompt() {
            bail!("--interactive requires a TTY");
        }

        let items = vec![
            ("Python", "Python runtime + pip"),
            ("UV", "uv (Python package manager)"),
            ("Go", "Go toolchain"),
            ("Node", "Node.js"),
            ("pnpm", "pnpm (via corepack, requires Node)"),
            ("Desktop", "webtop sidecar in compose (profile: desktop)"),
        ];
        let labels: Vec<String> = items.iter().map(|(a, b)| format!("{a} - {b}")).collect();
        let selection = dialoguer::MultiSelect::with_theme(&ColorfulTheme::default())
            .with_prompt("Select components for this template")
            .items(&labels)
            .interact()
            .context("TUI selection failed")?;

        for idx in selection {
            match idx {
                0 => spec.python = true,
                1 => spec.uv = true,
                2 => spec.go = true,
                3 => spec.node = true,
                4 => spec.pnpm = true,
                5 => spec.desktop = true,
                _ => {}
            }
        }
    }

    for c in &args.with_components {
        match c {
            StackComponent::Python => spec.python = true,
            StackComponent::Uv => spec.uv = true,
            StackComponent::Go => spec.go = true,
            StackComponent::Node => spec.node = true,
            StackComponent::Pnpm => spec.pnpm = true,
            StackComponent::Desktop => spec.desktop = true,
        }
    }

    spec.normalize();

    let dir = match templates::write_composed_preset(&args.name, spec.clone(), args.force) {
        Ok(d) => d,
        Err(e)
            if !args.force
                && can_prompt()
                && e.downcast_ref::<templates::ForceRequired>().is_some() =>
        {
            let ok = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!(
                    "Template {} already exists. Overwrite? (equivalent to --force)",
                    args.name
                ))
                .default(false)
                .interact()
                .context("Prompt failed")?;
            if !ok {
                println!("Cancelled. Left existing template {}", args.name);
                return Ok(());
            }
            templates::write_composed_preset(&args.name, spec, true)?
        }
        Err(e) => return Err(e),
    };

    println!("Wrote composed template into {}", dir.display());
    Ok(())
}

fn cmd_init(args: InitArgs) -> Result<()> {
    let dir = require_existing_dir(&args.dir)?;
    copy_preset(&args.preset, &dir, args.force)?;
    println!(
        "Initialized: {} (preset: {})",
        dir.join(".devcontainer").display(),
        args.preset
    );
    Ok(())
}

fn cmd_up(args: UpArgs) -> Result<()> {
    let dir = require_existing_dir(&args.dir)?;

    let has_config = workspace_has_devcontainer_config(&dir);
    if !has_config && args.init {
        copy_preset(&args.preset, &dir, false)?;
    }
    ensure_in_path("devcontainer")?;

    if workspace_has_devcontainer_config(&dir) {
        let mut env = Vec::new();
        if args.desktop {
            env.push(("COMPOSE_PROFILES", "desktop".to_string()));
        }
        devcontainer_up(&dir, None, &env)?;
    } else {
        let compose_project = default_compose_project_name(&dir)?;
        devcontainer_up_stealth(
            &dir,
            &args.preset,
            args.force_env,
            &compose_project,
            "dc-cache",
            args.desktop,
        )?;
    }
    Ok(())
}

fn cmd_desktop_on(dir: PathBuf) -> Result<()> {
    let dir = require_existing_dir(&dir)?;
    ensure_in_path("devcontainer")?;

    if workspace_has_devcontainer_config(&dir) {
        devcontainer_up(&dir, None, &[("COMPOSE_PROFILES", "desktop".to_string())])?;
        if is_in_path("docker") {
            if let Some(url) = try_get_desktop_url_local(&dir)? {
                println!("Desktop URL: {url}");
            } else {
                println!("Desktop started. To get the URL:");
                println!(
                    "  (cd \"{}\" && docker compose -f compose.yaml port desktop 3000)",
                    dir.join(".devcontainer").display()
                );
            }
        }
        return Ok(());
    }

    let compose_project = default_compose_project_name(&dir)?;
    let (preset_dir, env) =
        devcontainer_up_stealth(&dir, "python-uv", false, &compose_project, "dc-cache", true)?;
    if is_in_path("docker") {
        if let Some(url) = try_get_desktop_url_from_compose(&preset_dir, &compose_project, &env)? {
            println!("Desktop URL: {url}");
        } else {
            println!("Desktop started. To get the URL:");
            println!(
                "  (cd \"{}\" && docker compose -p {} -f compose.yaml port desktop 3000)",
                preset_dir.display(),
                compose_project
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
    let cache_prefix = sanitize_compose(&repo_name);

    println!("Worktree: {}", worktree_dir.display());
    println!("Branch:   {branch_name}");
    println!("Compose:  {compose_project}");

    write_agent_meta(
        &args.agent_name,
        AgentMeta {
            preset: args.preset.clone(),
            compose_project: compose_project.clone(),
            cache_prefix: cache_prefix.clone(),
        },
    )?;

    if args.no_up {
        return Ok(());
    }

    ensure_in_path("devcontainer")?;
    let mut env = vec![
        ("COMPOSE_PROJECT_NAME", compose_project.clone()),
        ("DEVCONTAINER_CACHE_PREFIX", cache_prefix.clone()),
    ];
    if args.desktop {
        env.push(("COMPOSE_PROFILES", "desktop".to_string()));
    }

    if workspace_has_devcontainer_config(&worktree_dir) {
        devcontainer_up(&worktree_dir, None, &env)?;
    } else {
        devcontainer_up_stealth(
            &worktree_dir,
            &args.preset,
            args.force_env,
            &compose_project,
            &cache_prefix,
            args.desktop,
        )?;
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

    // Best-effort: ignore typical generated dirs so `git worktree remove` doesn't
    // require `--force` after normal devcontainer usage (e.g. uv creates .venv).
    ensure_git_exclude(&worktree_dir, ".devcontainer/")?;
    ensure_git_exclude(&worktree_dir, ".env")?;
    ensure_git_exclude(&worktree_dir, ".venv/")?;
    ensure_git_exclude(&worktree_dir, "node_modules/")?;
    ensure_git_exclude(&worktree_dir, "target/")?;
    ensure_git_exclude(&worktree_dir, ".pytest_cache/")?;
    ensure_git_exclude(&worktree_dir, ".ruff_cache/")?;

    let meta = read_agent_meta(&args.agent_name)?.unwrap_or_else(|| AgentMeta {
        preset: "python-uv".to_string(),
        compose_project: format!("agent_{}", sanitize_compose(&args.agent_name)),
        cache_prefix: sanitize_compose(&repo_name),
    });

    if let Err(e) = docker_compose_down_if_present(&worktree_dir) {
        eprintln!(
            "Warning: docker compose down failed for {}: {e:#}",
            worktree_dir.display()
        );
    }
    if !worktree_dir.join(".devcontainer").join("compose.yaml").exists() {
        if let Err(e) = docker_compose_down_stealth(&worktree_dir, &meta) {
            eprintln!(
                "Warning: docker compose down (stealth) failed for {}: {e:#}",
                worktree_dir.display()
            );
        }
    }
    let removed = git_worktree_remove(&worktree_dir, args.force)?;
    if !removed {
        println!(
            "Cancelled. Worktree not removed: {}",
            worktree_dir.display()
        );
        return Ok(());
    }
    // Do not delete the agent branch by default; removing the worktree is enough.
    // Users can delete the branch manually if desired.

    remove_agent_meta(&args.agent_name)?;

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
    } else if needs_overwrite {
        if can_prompt() {
            let ok = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(format!(
                    "Some files already exist under {}. Overwrite them? (equivalent to --force)",
                    devcontainer_dir.display()
                ))
                .default(false)
                .interact()
                .context("Prompt failed")?;
            if !ok {
                println!("Cancelled. Left existing {}", devcontainer_dir.display());
                return Ok(());
            }
            true
        } else {
            bail!(
                "Some files already exist under {}. Use --force to overwrite.",
                devcontainer_dir.display()
            );
        }
    } else {
        false
    };

    for (name, contents) in files {
        let target = devcontainer_dir.join(&name);
        if target.exists() && !overwrite_all {
            bail!(
                "{} already exists. Use --force to overwrite.",
                target.display()
            );
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

fn devcontainer_up(
    dir: &Path,
    override_config: Option<&Path>,
    env: &[(&str, String)],
) -> Result<()> {
    let mut cmd = Command::new("devcontainer");
    cmd.arg("up").arg("--workspace-folder").arg(dir);
    if let Some(cfg) = override_config {
        cmd.arg("--override-config").arg(cfg);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    run_ok(cmd).context("devcontainer up failed")?;
    Ok(())
}

fn devcontainer_up_stealth(
    dir: &Path,
    preset: &str,
    force_runtime: bool,
    compose_project: &str,
    cache_prefix: &str,
    desktop: bool,
) -> Result<(PathBuf, Vec<(&'static str, String)>)> {
    let abs = std::fs::canonicalize(dir)
        .with_context(|| format!("Failed to resolve directory: {}", dir.display()))?;

    let dc_dir = templates::ensure_runtime_preset_stealth(preset, force_runtime)?;
    let dc_json = dc_dir.join("devcontainer.json");

    let mut env = vec![
        ("PC_WORKSPACE_DIR", abs.to_string_lossy().to_string()),
        ("PC_DEVCONTAINER_DIR", dc_dir.to_string_lossy().to_string()),
        ("COMPOSE_PROJECT_NAME", compose_project.to_string()),
        ("DEVCONTAINER_CACHE_PREFIX", cache_prefix.to_string()),
    ];
    if desktop {
        env.push(("COMPOSE_PROFILES", "desktop".to_string()));
    }

    devcontainer_up(&abs, Some(&dc_json), &env)?;
    Ok((dc_dir, env))
}

fn workspace_has_devcontainer_config(dir: &Path) -> bool {
    dir.join(".devcontainer").join("devcontainer.json").exists()
        || dir.join(".devcontainer.json").exists()
}

fn default_compose_project_name(dir: &Path) -> Result<String> {
    let abs = std::fs::canonicalize(dir)
        .with_context(|| format!("Failed to resolve directory: {}", dir.display()))?;
    let base = abs
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("workspace");
    Ok(format!(
        "dc_{}_{}",
        sanitize_compose(base),
        short_hash(&abs)
    ))
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

fn write_agent_meta(agent_name: &str, meta: AgentMeta) -> Result<()> {
    let path = agent_meta_path(agent_name)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(&meta)? + "\n";
    std::fs::write(&path, text).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn read_agent_meta(agent_name: &str) -> Result<Option<AgentMeta>> {
    let path = agent_meta_path(agent_name)?;
    if !path.exists() {
        return Ok(None);
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))?;
    Ok(Some(serde_json::from_str::<AgentMeta>(&text)?))
}

fn remove_agent_meta(agent_name: &str) -> Result<()> {
    let path = agent_meta_path(agent_name)?;
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    Ok(())
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
    if force {
        run_ok(Command::new("git").args(["worktree", "remove", "--force"]).arg(path))
            .context("git worktree remove failed")?;
        return Ok(true);
    }
    git_worktree_remove_interactive(path)
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
        if let Ok(p) = git_status_porcelain(path) {
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

fn git_status_porcelain(worktree_dir: &Path) -> Result<String> {
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
    // Keep volumes by default; cache volumes are often shared across agents.
    cmd.args(["down", "--remove-orphans"]);

    let status = cmd
        .status()
        .context("Failed to spawn docker compose down")?;
    if !status.success() {
        bail!("docker compose down failed with status: {status}");
    }
    Ok(())
}

fn docker_compose_down_stealth(worktree_dir: &Path, meta: &AgentMeta) -> Result<()> {
    if !is_in_path("docker") {
        return Ok(());
    }
    let abs = std::fs::canonicalize(worktree_dir)
        .with_context(|| format!("Failed to resolve directory: {}", worktree_dir.display()))?;

    let dc_dir = templates::ensure_runtime_preset_stealth(&meta.preset, false)?;
    let mut cmd = Command::new("docker");
    cmd.current_dir(&dc_dir)
        .args([
            "compose",
            "-p",
            &meta.compose_project,
            "-f",
            "compose.yaml",
            "down",
            "--remove-orphans",
        ])
        .env("PC_WORKSPACE_DIR", abs.to_string_lossy().to_string())
        .env("PC_DEVCONTAINER_DIR", dc_dir.to_string_lossy().to_string())
        .env("COMPOSE_PROJECT_NAME", meta.compose_project.clone())
        .env("DEVCONTAINER_CACHE_PREFIX", meta.cache_prefix.clone())
        .env("COMPOSE_PROFILES", "desktop");

    let status = cmd
        .status()
        .context("Failed to spawn docker compose down (stealth)")?;
    if !status.success() {
        bail!("docker compose down (stealth) failed with status: {status}");
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

fn try_get_desktop_url_local(dir: &Path) -> Result<Option<String>> {
    let dc_dir = dir.join(".devcontainer");
    let output = Command::new("docker")
        .current_dir(&dc_dir)
        .env("COMPOSE_PROFILES", "desktop")
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

fn try_get_desktop_url_from_compose(
    dc_dir: &Path,
    compose_project: &str,
    env: &[(&str, String)],
) -> Result<Option<String>> {
    let mut cmd = Command::new("docker");
    cmd.current_dir(dc_dir).args([
        "compose",
        "-p",
        compose_project,
        "-f",
        "compose.yaml",
        "port",
        "desktop",
        "3000",
    ]);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let output = cmd.output().context("Failed to run docker compose port")?;
    if !output.status.success() {
        return Ok(None);
    }
    let s = String::from_utf8_lossy(&output.stdout);
    let mapping = s.trim();
    if mapping.is_empty() {
        return Ok(None);
    }
    let (host, port) = mapping
        .rsplit_once(':')
        .ok_or_else(|| anyhow!("Unexpected docker port output: {mapping:?}"))?;
    let host = if host == "0.0.0.0" { "127.0.0.1" } else { host };
    Ok(Some(format!("http://{host}:{port}/")))
}
