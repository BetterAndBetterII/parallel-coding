#[cfg(unix)]
#[path = "common/mod.rs"]
mod common;

#[cfg(unix)]
mod unix_only {
    use std::fs;

    use assert_cmd::Command;
    use predicates::str::contains;
    use tempfile::TempDir;

    use super::common;

    #[test]
    fn pc_up_normal_mode_calls_devcontainer_without_config() {
        let td = TempDir::new().unwrap();
        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();
        let ws = td.path().join("ws");
        fs::create_dir_all(ws.join(".devcontainer")).unwrap();
        fs::write(ws.join(".devcontainer").join("devcontainer.json"), "{}\n").unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();
        let log = td.path().join("devcontainer.log");

        common::write_executable(
            &stub_bin,
            "devcontainer",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "devcontainer 0.0"
  exit 0
fi
echo "ARGS:$@" >> "$PC_LOG"
exit 0
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .env("PC_HOME", &pc_home)
            .env("PATH", stub_bin.to_string_lossy().to_string())
            .env("PC_LOG", &log)
            .args(["up", ws.to_str().unwrap()])
            .assert()
            .success();

        let text = fs::read_to_string(&log).unwrap();
        assert!(text.contains("ARGS:up --workspace-folder"));
        assert!(!text.contains("--config"));
    }

    #[test]
    fn pc_up_stealth_mode_uses_config_flag() {
        let td = TempDir::new().unwrap();
        let ws = td.path().join("ws");
        fs::create_dir_all(&ws).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();
        let log = td.path().join("devcontainer.log");

        common::write_executable(
            &stub_bin,
            "devcontainer",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "devcontainer 0.0"
  exit 0
fi
echo "ARGS:$@" >> "$PC_LOG"
echo "COMPOSE_PROFILES=$COMPOSE_PROFILES" >> "$PC_LOG"
exit 0
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .env("PC_HOME", &pc_home)
            .env("PATH", stub_bin.to_string_lossy().to_string())
            .env("PC_LOG", &log)
            .args(["up", ws.to_str().unwrap()])
            .assert()
            .success();

        let text = fs::read_to_string(&log).unwrap();
        assert!(text.contains("ARGS:up --workspace-folder"));
        assert!(text.contains("--config"));
        assert!(
            text.contains("runtime/devcontainer-presets/python-uv/.devcontainer/devcontainer.json")
        );
    }

    #[test]
    fn pc_up_desktop_sets_compose_profiles_in_stealth_mode() {
        let td = TempDir::new().unwrap();
        let ws = td.path().join("ws");
        fs::create_dir_all(&ws).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();
        let log = td.path().join("devcontainer.log");

        common::write_executable(
            &stub_bin,
            "devcontainer",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "devcontainer 0.0"
  exit 0
fi
echo "COMPOSE_PROFILES=$COMPOSE_PROFILES" >> "$PC_LOG"
exit 0
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .env("PC_HOME", &pc_home)
            .env("PATH", stub_bin.to_string_lossy().to_string())
            .env("PC_LOG", &log)
            .args(["up", ws.to_str().unwrap(), "--desktop"])
            .assert()
            .success();

        let text = fs::read_to_string(&log).unwrap();
        assert!(text.contains("COMPOSE_PROFILES=desktop"));
    }

    #[test]
    fn pc_up_init_creates_devcontainer_files_then_runs_normal_mode() {
        let td = TempDir::new().unwrap();
        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();
        let ws = td.path().join("ws");
        fs::create_dir_all(&ws).unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();
        let log = td.path().join("devcontainer.log");

        common::write_executable(
            &stub_bin,
            "devcontainer",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "devcontainer 0.0"
  exit 0
fi
echo "ARGS:$@" >> "$PC_LOG"
exit 0
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .env("PC_HOME", &pc_home)
            .env("PATH", stub_bin.to_string_lossy().to_string())
            .env("PC_LOG", &log)
            .args(["up", ws.to_str().unwrap(), "--init"])
            .assert()
            .success();

        assert!(ws.join(".devcontainer").join("devcontainer.json").exists());
        assert!(ws.join(".devcontainer").join("compose.yaml").exists());
        assert!(ws.join(".devcontainer").join("Dockerfile").exists());

        let text = fs::read_to_string(&log).unwrap();
        assert!(!text.contains("--config"), "should not use stealth mode");
    }

    #[test]
    fn pc_up_propagates_devcontainer_stderr_and_exit_code() {
        let td = TempDir::new().unwrap();
        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();
        let ws = td.path().join("ws");
        fs::create_dir_all(&ws).unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();

        common::write_executable(
            &stub_bin,
            "devcontainer",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "devcontainer 0.0"
  exit 0
fi
echo "boom from devcontainer" 1>&2
exit 42
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .env("PC_HOME", &pc_home)
            .env("PATH", stub_bin.to_string_lossy().to_string())
            .args(["up", ws.to_str().unwrap()])
            .assert()
            .failure()
            .stderr(contains("exit 42"))
            .stderr(contains("boom from devcontainer"));
    }

    #[test]
    fn pc_up_errors_when_devcontainer_missing() {
        let td = TempDir::new().unwrap();
        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();
        let ws = td.path().join("ws");
        fs::create_dir_all(&ws).unwrap();

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .env("PC_HOME", &pc_home)
            .env("PATH", "")
            .args(["up", ws.to_str().unwrap()])
            .assert()
            .failure()
            .stderr(contains("devcontainer not found in PATH"));
    }
}
