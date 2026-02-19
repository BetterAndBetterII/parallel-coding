use std::fs;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

#[test]
fn templates_init_installs_embedded_presets_into_pc_home() {
    let td = TempDir::new().unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", td.path())
        .args(["templates", "init", "--non-interactive"])
        .assert()
        .success();

    let base = td.path().join("templates").join("python-uv");
    assert!(base.join("devcontainer.json").exists());
    assert!(base.join("compose.yaml").exists());
    assert!(base.join("Dockerfile").exists());
}

#[test]
fn templates_init_is_not_idempotent_without_force_in_non_tty() {
    let td = TempDir::new().unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", td.path())
        .args(["templates", "init", "--non-interactive"])
        .assert()
        .success();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", td.path())
        .args(["templates", "init", "--non-interactive"])
        .assert()
        .failure()
        .stderr(contains("Target exists"));
}

#[test]
fn templates_compose_creates_named_template() {
    let td = TempDir::new().unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", td.path())
        .args([
            "templates",
            "compose",
            "mix",
            "--with",
            "python",
            "--with",
            "uv",
            "--with",
            "node",
        ])
        .assert()
        .success();

    let base = td.path().join("templates").join("mix");
    let dc = fs::read_to_string(base.join("devcontainer.json")).unwrap();
    assert!(dc.contains("features"));
    assert!(dc.contains("devcontainers/features/python"));
    assert!(dc.contains("devcontainers/features/node"));
}
