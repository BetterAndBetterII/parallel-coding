use std::path::Path;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use tempfile::TempDir;

fn run_git(repo: &Path, args: &[&str]) {
    let status = StdCommand::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("spawn git");
    assert!(status.success(), "git {:?} failed", args);
}

fn init_repo(repo: &Path) {
    std::fs::create_dir_all(repo).unwrap();
    run_git(repo, &["init", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "hello\n").unwrap();
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

#[test]
fn agent_new_and_rm_clean_should_not_require_force() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    init_repo(&repo);

    let agents = td.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "agent-a",
            "--no-up",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success();

    let worktree = agents.join("agent-a");
    assert!(worktree.exists(), "worktree dir should exist");

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "rm",
            "agent-a",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(!worktree.exists(), "worktree dir should be removed");

    let status = StdCommand::new("git")
        .current_dir(&repo)
        .args(["show-ref", "--verify", "--quiet", "refs/heads/agent-a"])
        .status()
        .unwrap();
    assert!(status.success(), "agent branch should remain");
}

#[test]
fn agent_rm_without_force_should_fail_if_user_left_untracked_files() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    init_repo(&repo);

    let agents = td.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "agent-a",
            "--no-up",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success();

    let worktree = agents.join("agent-a");
    std::fs::write(worktree.join("leftover.txt"), "x").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "rm",
            "agent-a",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .failure();
}

#[test]
fn agent_rm_should_succeed_with_common_generated_dirs() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    init_repo(&repo);

    let agents = td.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "agent-a",
            "--no-up",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success();

    let worktree = agents.join("agent-a");
    std::fs::create_dir_all(worktree.join(".venv")).unwrap();
    std::fs::write(worktree.join(".venv").join("pyvenv.cfg"), "x").unwrap();
    std::fs::create_dir_all(worktree.join("node_modules")).unwrap();
    std::fs::write(worktree.join("node_modules").join(".keep"), "x").unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "rm",
            "agent-a",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert!(!worktree.exists(), "worktree dir should be removed");
}

#[test]
fn agent_new_should_refuse_when_worktree_exists() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    init_repo(&repo);

    let agents = td.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "agent-a",
            "--no-up",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "agent-a",
            "--no-up",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .failure();
}

fn parse_worktree_from_stdout(stdout: &[u8]) -> std::path::PathBuf {
    let s = String::from_utf8_lossy(stdout);
    let line = s
        .lines()
        .find(|l| l.starts_with("Worktree: "))
        .unwrap_or_else(|| panic!("missing Worktree line in stdout:\n{s}"));
    std::path::PathBuf::from(line.trim_start_matches("Worktree: ").trim())
}

#[test]
fn agent_new_accepts_branch_names_with_slash() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    init_repo(&repo);

    let agents = td.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();

    let output = Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "feat/tui-templates",
            "--no-up",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "pc agent new failed: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let worktree = parse_worktree_from_stdout(&output.stdout);
    assert!(worktree.exists(), "worktree dir should exist");
    assert!(
        worktree.starts_with(&agents),
        "worktree should be under base-dir"
    );

    let status = StdCommand::new("git")
        .current_dir(&repo)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            "refs/heads/feat/tui-templates",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "branch should exist");
}

#[test]
fn top_level_new_is_alias_of_agent_new() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    init_repo(&repo);

    let agents = td.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();

    let output = Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "new",
            "feat/pc-new",
            "--no-up",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "pc new failed: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let worktree = parse_worktree_from_stdout(&output.stdout);
    assert!(worktree.exists(), "worktree dir should exist");

    let status = StdCommand::new("git")
        .current_dir(&repo)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            "refs/heads/feat/pc-new",
        ])
        .status()
        .unwrap();
    assert!(status.success(), "branch should exist");
}
