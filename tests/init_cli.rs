use std::fs;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

#[test]
fn pc_init_writes_devcontainer_from_embedded_preset() {
    let td = TempDir::new().unwrap();
    let pc_home = td.path().join("pc-home");
    fs::create_dir_all(&pc_home).unwrap();
    let dir = td.path().join("ws");
    fs::create_dir_all(&dir).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", &pc_home)
        .args(["init", dir.to_str().unwrap(), "--preset", "python-uv"])
        .assert()
        .success();

    let dc = fs::read_to_string(dir.join(".devcontainer").join("devcontainer.json")).unwrap();
    let compose = fs::read_to_string(dir.join(".devcontainer").join("compose.yaml")).unwrap();
    let dockerfile = fs::read_to_string(dir.join(".devcontainer").join("Dockerfile")).unwrap();

    assert!(dc.contains("\"dockerComposeFile\"") || dc.contains("\"service\""));
    assert!(compose.contains("services:"));
    assert!(dockerfile.starts_with("FROM "));
}

#[test]
fn pc_init_is_not_idempotent_without_force_in_non_tty() {
    let td = TempDir::new().unwrap();
    let pc_home = td.path().join("pc-home");
    fs::create_dir_all(&pc_home).unwrap();
    let dir = td.path().join("ws");
    fs::create_dir_all(&dir).unwrap();

    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("pc"));
    cmd.env("PC_HOME", &pc_home);
    cmd.args(["init", dir.to_str().unwrap(), "--preset", "python-uv"]);
    cmd.assert().success();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", &pc_home)
        .args(["init", dir.to_str().unwrap(), "--preset", "python-uv"])
        .assert()
        .failure()
        .stderr(contains("Use --force"));
}

#[test]
fn pc_init_force_overwrites_existing_files() {
    let td = TempDir::new().unwrap();
    let pc_home = td.path().join("pc-home");
    fs::create_dir_all(&pc_home).unwrap();
    let dir = td.path().join("ws");
    fs::create_dir_all(dir.join(".devcontainer")).unwrap();
    fs::write(
        dir.join(".devcontainer").join("devcontainer.json"),
        "SENTINEL\n",
    )
    .unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", &pc_home)
        .args([
            "init",
            dir.to_str().unwrap(),
            "--preset",
            "python-uv",
            "--force",
        ])
        .assert()
        .success();

    let dc = fs::read_to_string(dir.join(".devcontainer").join("devcontainer.json")).unwrap();
    assert!(!dc.contains("SENTINEL"));
}

#[test]
fn pc_init_prefers_pc_home_template_override() {
    let td = TempDir::new().unwrap();
    let pc_home = td.path().join("pc-home");
    let tpl = pc_home.join("templates").join("python-uv");
    fs::create_dir_all(&tpl).unwrap();
    fs::write(
        tpl.join("devcontainer.json"),
        "{ \"name\": \"OVERRIDE\" }\n",
    )
    .unwrap();
    fs::write(
        tpl.join("compose.yaml"),
        "services: { dev: { command: sleep infinity } }\n",
    )
    .unwrap();
    fs::write(tpl.join("Dockerfile"), "FROM scratch\n").unwrap();

    let dir = td.path().join("ws");
    fs::create_dir_all(&dir).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", &pc_home)
        .args(["init", dir.to_str().unwrap(), "--preset", "python-uv"])
        .assert()
        .success();

    let dc = fs::read_to_string(dir.join(".devcontainer").join("devcontainer.json")).unwrap();
    assert!(dc.contains("OVERRIDE"));
}

#[test]
fn pc_init_errors_on_unknown_preset() {
    let td = TempDir::new().unwrap();
    let pc_home = td.path().join("pc-home");
    fs::create_dir_all(&pc_home).unwrap();
    let dir = td.path().join("ws");
    fs::create_dir_all(&dir).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", &pc_home)
        .args(["init", dir.to_str().unwrap(), "--preset", "__no_such__"])
        .assert()
        .failure()
        .stderr(contains("Unknown preset"));
}

#[test]
fn pc_init_errors_when_pc_home_override_is_incomplete() {
    let td = TempDir::new().unwrap();
    let pc_home = td.path().join("pc-home");
    let tpl = pc_home.join("templates").join("python-uv");
    fs::create_dir_all(&tpl).unwrap();
    fs::write(
        tpl.join("devcontainer.json"),
        "{ \"name\": \"OVERRIDE\" }\n",
    )
    .unwrap();
    // Missing compose.yaml / Dockerfile

    let dir = td.path().join("ws");
    fs::create_dir_all(&dir).unwrap();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", &pc_home)
        .args(["init", dir.to_str().unwrap(), "--preset", "python-uv"])
        .assert()
        .failure()
        .stderr(contains("Failed to read"));
}
