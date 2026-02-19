use std::fs;

use assert_cmd::Command;
use tempfile::TempDir;

#[test]
fn templates_compose_writes_only_selected_stacks() {
    let td = TempDir::new().unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", td.path())
        .args([
            "templates",
            "compose",
            "py",
            "--with",
            "python",
            "--with",
            "uv",
        ])
        .assert()
        .success();

    let base = td.path().join("templates").join("py");
    let devcontainer = fs::read_to_string(base.join("devcontainer.json")).unwrap();
    let compose = fs::read_to_string(base.join("compose.yaml")).unwrap();

    assert!(devcontainer.contains("features"));
    assert!(devcontainer.contains("devcontainers/features/python"));
    assert!(!devcontainer.contains("devcontainers/features/go"));
    assert!(!devcontainer.contains("devcontainers/features/node"));
    assert!(devcontainer.contains("pip install --user uv"));

    assert!(compose.contains("uv_cache"));
    assert!(compose.contains("pip_cache"));
    assert!(!compose.contains("go_mod_cache"));
    assert!(!compose.contains("pnpm_home"));
}

#[test]
fn templates_compose_pnpm_implies_node() {
    let td = TempDir::new().unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", td.path())
        .args(["templates", "compose", "js", "--with", "pnpm"])
        .assert()
        .success();

    let base = td.path().join("templates").join("js");
    let devcontainer = fs::read_to_string(base.join("devcontainer.json")).unwrap();
    assert!(devcontainer.contains("devcontainers/features/node"));
}

