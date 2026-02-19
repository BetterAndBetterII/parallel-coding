mod cli;
mod commands;
mod exec;
mod git;
mod meta;
mod vscode;

fn main() -> anyhow::Result<()> {
    crate::cli::run()
}
