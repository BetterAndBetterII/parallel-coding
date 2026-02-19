#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub fn run_git(repo: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("spawn git");
    assert!(status.success(), "git {:?} failed", args);
}

pub fn init_repo(repo: &Path) {
    fs::create_dir_all(repo).unwrap();
    run_git(repo, &["init", "-b", "main"]);
    fs::write(repo.join("README.md"), "hello\n").unwrap();
    run_git(repo, &["add", "-A"]);
    run_git(
        repo,
        &[
            "-c",
            "user.name=pc-test",
            "-c",
            "user.email=pc-test@example.com",
            "commit",
            "-m",
            "init",
        ],
    );
}

#[cfg(unix)]
pub fn write_executable(dir: &Path, name: &str, script: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, script).unwrap();
    let mut perms = fs::metadata(&path).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).unwrap();
    path
}

pub fn prepend_path(stub_bin: &Path) -> String {
    let old = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", stub_bin.display(), old)
}
