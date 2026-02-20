use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::commands;

#[derive(Parser, Debug)]
#[command(name = "pc", version, about = "Parallel coding helper (git worktree)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a git worktree + branch
    New(NewArgs),
    /// Remove a worktree (git worktree remove)
    Rm(RmArgs),
    /// Backward-compatible alias (hidden)
    #[command(hide = true)]
    Agent(AgentArgs),
}

#[derive(Args, Debug)]
struct AgentArgs {
    #[command(subcommand)]
    command: AgentCommands,
}

#[derive(Subcommand, Debug)]
enum AgentCommands {
    /// Create a git worktree + branch
    New(NewArgs),
    /// Remove a worktree (git worktree remove)
    Rm(RmArgs),
}

#[derive(Args, Debug)]
pub(crate) struct NewArgs {
    /// Branch name to create/use (can include `/`, e.g. `feat/tui-templates`).
    /// If omitted (TTY only), a TUI selector will be shown.
    pub(crate) branch_name: Option<String>,
    /// Override the derived agent name (used for worktree directory and metadata lookup)
    #[arg(long = "agent-name")]
    pub(crate) agent_name: Option<String>,
    /// Base branch/ref for the new worktree branch (default: current HEAD).
    /// Pass `--base` without a value to select interactively (TTY only).
    #[arg(long, num_args = 0..=1, default_missing_value = "__tui__")]
    pub(crate) base: Option<String>,
    /// Select base branch with an interactive TUI (sorted by recent updates)
    #[arg(long)]
    pub(crate) select_base: bool,
    /// Base directory to place worktrees
    #[arg(long)]
    pub(crate) base_dir: Option<PathBuf>,
    /// Do not open VS Code in a new window
    #[arg(long)]
    pub(crate) no_open: bool,
}

#[derive(Args, Debug)]
pub(crate) struct RmArgs {
    /// Branch name (or agent name) to remove
    pub(crate) branch_name: String,
    /// Override the derived agent name (used for default worktree path and metadata lookup)
    #[arg(long = "agent-name")]
    pub(crate) agent_name: Option<String>,
    /// Base directory to place worktrees (for locating existing worktree dir)
    #[arg(long)]
    pub(crate) base_dir: Option<PathBuf>,
    /// Force removal (passes --force to git worktree remove)
    #[arg(long)]
    pub(crate) force: bool,
}

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::New(args) => commands::agent::cmd_new(args),
        Commands::Rm(args) => commands::agent::cmd_rm(args),
        Commands::Agent(args) => match args.command {
            AgentCommands::New(a) => commands::agent::cmd_new(a),
            AgentCommands::Rm(a) => commands::agent::cmd_rm(a),
        },
    }
}
