use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};

pub(crate) fn ensure_in_path(bin: &str) -> Result<()> {
    if is_in_path(bin) {
        Ok(())
    } else {
        bail!("{bin} not found in PATH");
    }
}

pub(crate) fn is_in_path(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub(crate) fn run_ok(mut cmd: Command) -> Result<ExitStatus> {
    let status = cmd.status().context("Failed to spawn command")?;
    if status.success() {
        Ok(status)
    } else {
        bail!("Command failed with status: {status}");
    }
}

pub(crate) fn can_prompt() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}
