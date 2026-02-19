use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

use anyhow::{anyhow, bail, Context, Result};
use clap::{Args, Parser, Subcommand};

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
}

#[derive(Args, Debug)]
struct AgentNewArgs {
    /// Agent name (used in branch name and compose project name)
    agent_name: String,
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
        let dir = templates::install_embedded_preset(preset, args.force)?;
        println!("Installed preset {preset} into {}", dir.display());
    }
    println!("Edit templates under $HOME/.pc/templates/<preset>/ to customize.");
    println!("Tip: set PC_HOME=/some/dir to override $HOME/.pc.");
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
    git_worktree_add(&worktree_dir, &branch_name)?;
    let worktree_dir = std::fs::canonicalize(&worktree_dir)
        .with_context(|| format!("Failed to resolve worktree dir: {}", worktree_dir.display()))?;

    println!("Worktree: {}", worktree_dir.display());
    println!("Branch:   {branch_name}");

    if args.no_up {
        return Ok(());
    }

    ensure_in_path("devcontainer")?;
    if workspace_has_devcontainer_config(&worktree_dir) {
        devcontainer_up(&worktree_dir, None, &[])?;
        if args.desktop {
            devcontainer_up(
                &worktree_dir,
                None,
                &[("COMPOSE_PROFILES", "desktop".to_string())],
            )?;
        }
    } else {
        let compose_project = format!("agent_{}", sanitize_compose(&args.agent_name));
        println!("Compose:  {compose_project}");
        devcontainer_up_stealth(
            &worktree_dir,
            "python-uv",
            args.force_env,
            &compose_project,
            &sanitize_compose(&repo_name),
            false,
        )?;
        if args.desktop {
            devcontainer_up_stealth(
                &worktree_dir,
                "python-uv",
                args.force_env,
                &compose_project,
                &sanitize_compose(&repo_name),
                true,
            )?;
        }
    }

    if !args.no_open && is_in_path("code") {
        let _ = Command::new("code")
            .arg("--new-window")
            .arg(&worktree_dir)
            .status();
    }

    Ok(())
}

fn copy_preset(preset: &str, dir: &Path, force: bool) -> Result<()> {
    let files = templates::preset_files(preset)?;

    let devcontainer_dir = dir.join(".devcontainer");
    std::fs::create_dir_all(&devcontainer_dir)
        .with_context(|| format!("Failed to create {}", devcontainer_dir.display()))?;

    for (name, contents) in files {
        let target = devcontainer_dir.join(&name);
        if target.exists() && !force {
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
        .status()
        .context("Failed to run git rev-parse --verify HEAD")?;
    Ok(status.success())
}

fn git_worktree_add(worktree_dir: &Path, branch_name: &str) -> Result<()> {
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
            .arg("HEAD");
    }
    run_ok(cmd).context("git worktree add failed")?;
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
