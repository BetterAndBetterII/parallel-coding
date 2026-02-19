use std::fs;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

#[path = "common/mod.rs"]
mod common;

#[test]
fn help_mentions_new_subcommand() {
    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("new").or(contains("New")));
}

#[test]
fn agent_new_rejects_invalid_branch_names() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    common::init_repo(&repo);

    let agents = td.path().join("agents");
    fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "bad branch",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains("Invalid branch name"));

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "@{",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains("Invalid branch name"));

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
            "--",
            "-bad",
        ])
        .assert()
        .failure()
        .stderr(contains("Invalid branch name"));
}

#[test]
fn agent_new_derives_agent_name_for_branch_with_slash() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    common::init_repo(&repo);

    let agents = td.path().join("agents");
    fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "feat/a",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("Agent:    feat_a"));

    assert!(agents.join("feat_a").exists());
}

#[test]
fn agent_new_agent_name_override_controls_worktree_dir() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    common::init_repo(&repo);

    let agents = td.path().join("agents");
    fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "feat/a",
            "--agent-name",
            "agent-a",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(contains("Agent:    agent-a"));

    assert!(agents.join("agent-a").exists());
}

#[test]
fn pc_new_alias_works() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    common::init_repo(&repo);

    let agents = td.path().join("agents");
    fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "new",
            "feat/a",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .success();
}

#[test]
fn agent_new_rejects_dot_agent_name() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    common::init_repo(&repo);

    let agents = td.path().join("agents");
    fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "feat/a",
            "--agent-name",
            ".",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains("cannot be '.'"));
}

#[test]
fn agent_new_detects_agent_name_collisions() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    common::init_repo(&repo);

    let agents = td.path().join("agents");
    fs::create_dir_all(&agents).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            "feat/a",
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
            "feat_a",
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains("already exists"));
}

#[test]
fn agent_new_errors_when_derived_agent_name_is_too_long() {
    let td = TempDir::new().unwrap();
    let repo = td.path().join("repo");
    common::init_repo(&repo);

    let agents = td.path().join("agents");
    fs::create_dir_all(&agents).unwrap();

    let branch = format!("feat/{}", "a".repeat(100));

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .current_dir(&repo)
        .args([
            "agent",
            "new",
            &branch,
            "--no-open",
            "--base-dir",
            agents.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(contains("--agent-name"));
}
