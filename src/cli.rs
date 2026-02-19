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
    /// Create a git worktree + branch (alias of `pc agent new`)
    New(AgentNewArgs),
    /// Git worktree based agent workflows
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
    New(AgentNewArgs),
    /// Remove an agent (git worktree remove)
    Rm(AgentRmArgs),
}

#[derive(Args, Debug)]
pub(crate) struct AgentNewArgs {
    /// Branch name to create/use (can include `/`, e.g. `feat/tui-templates`)
    pub(crate) branch_name: String,
    /// Override the derived agent name (used for worktree directory and metadata lookup)
    #[arg(long = "agent-name")]
    pub(crate) agent_name: Option<String>,
    /// Base branch/ref for the new worktree branch (default: current HEAD)
    #[arg(long)]
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
pub(crate) struct AgentRmArgs {
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
        Commands::New(args) => commands::agent::cmd_agent_new(args),
        Commands::Agent(args) => match args.command {
            AgentCommands::New(a) => commands::agent::cmd_agent_new(a),
            AgentCommands::Rm(a) => commands::agent::cmd_agent_rm(a),
        },
    }
}
