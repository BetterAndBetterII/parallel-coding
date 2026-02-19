#[cfg(unix)]
mod unix_only {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
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

    fn write_executable(dir: &Path, name: &str, script: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, script).unwrap();
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&path, perms).unwrap();
        path
    }

    fn prepend_path(stub_bin: &Path) -> String {
        let old = std::env::var("PATH").unwrap_or_default();
        format!("{}:{}", stub_bin.display(), old)
    }

    #[test]
    fn agent_new_can_be_tested_with_mocked_devcontainer_and_docker() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();

        let devcontainer_log = td.path().join("devcontainer.log");
        let docker_volumes = td.path().join("docker-volumes.log");
        let docker_log = td.path().join("docker.log");

        write_executable(
            &stub_bin,
            "devcontainer",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "devcontainer 0.0"
  exit 0
fi
echo "ARGS:$@" >> "$PC_DEVCONTAINER_LOG"
echo "COMPOSE_PROJECT_NAME=$COMPOSE_PROJECT_NAME" >> "$PC_DEVCONTAINER_LOG"
echo "DEVCONTAINER_CACHE_PREFIX=$DEVCONTAINER_CACHE_PREFIX" >> "$PC_DEVCONTAINER_LOG"
exit 0
"#,
        );

        write_executable(
            &stub_bin,
            "docker",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Docker version 0.0"
  exit 0
fi
if [ "$1" = "volume" ] && [ "$2" = "create" ]; then
  echo "$3" >> "$PC_DOCKER_VOLUMES"
  exit 0
fi
echo "ARGS:$@" >> "$PC_DOCKER_LOG"
exit 0
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .env("PC_DEVCONTAINER_LOG", &devcontainer_log)
            .env("PC_DOCKER_VOLUMES", &docker_volumes)
            .env("PC_DOCKER_LOG", &docker_log)
            .env("PATH", prepend_path(&stub_bin))
            .args([
                "agent",
                "new",
                "agent-a",
                "--no-open",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .success();

        let worktree = agents.join("agent-a");
        assert!(worktree.exists(), "worktree dir should exist");
        assert!(
            worktree.join(".devcontainer").join("devcontainer.json").exists(),
            "agent new should materialize .devcontainer/devcontainer.json in the worktree"
        );
        assert!(
            worktree.join(".devcontainer").join(".env").exists(),
            "agent new should write .devcontainer/.env for stable compose project/env"
        );

        let dc_text = fs::read_to_string(&devcontainer_log).unwrap();
        assert!(
            dc_text.contains("ARGS:up --workspace-folder"),
            "devcontainer up should be invoked: {dc_text}"
        );
        assert!(
            !dc_text.contains("--config"),
            "agent new should use worktree .devcontainer (no --config): {dc_text}"
        );
        assert!(
            dc_text.contains("COMPOSE_PROJECT_NAME=agent_agent_a"),
            "compose project should be passed via env: {dc_text}"
        );
        assert!(
            dc_text.contains("DEVCONTAINER_CACHE_PREFIX=repo"),
            "cache prefix should be repo name: {dc_text}"
        );

        let vols: Vec<String> = fs::read_to_string(&docker_volumes)
            .unwrap()
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        for expected in [
            "repo-uv-cache",
            "repo-pip-cache",
            "repo-pnpm-home",
            "repo-npm-cache",
            "repo-vscode-extensions",
        ] {
            assert!(
                vols.iter().any(|v| v == expected),
                "expected docker volume create {expected}, got: {vols:?}"
            );
        }
    }

    #[test]
    fn agent_rm_runs_docker_compose_down_without_volumes_flag() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

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
        let dc_dir = worktree.join(".devcontainer");
        fs::create_dir_all(&dc_dir).unwrap();
        fs::write(dc_dir.join("compose.yaml"), "services: {}\n").unwrap();
        fs::write(dc_dir.join(".env"), "COMPOSE_PROJECT_NAME=agent_agent_a\n").unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();
        let docker_log = td.path().join("docker.log");

        write_executable(
            &stub_bin,
            "docker",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Docker version 0.0"
  exit 0
fi
echo "ARGS:$@" >> "$PC_DOCKER_LOG"
exit 0
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_DOCKER_LOG", &docker_log)
            .env("PATH", prepend_path(&stub_bin))
            .args([
                "agent",
                "rm",
                "agent-a",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .success();

        let text = fs::read_to_string(&docker_log).unwrap();
        assert!(
            text.contains("ARGS:compose -f compose.yaml --env-file .env down --remove-orphans"),
            "docker compose down should be invoked with --env-file when present: {text}"
        );
        assert!(
            !text.contains(" -v ") && !text.contains("--volumes"),
            "should not remove volumes by default: {text}"
        );
    }

    #[test]
    fn agent_new_should_rollback_worktree_and_branch_on_failure() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();

        write_executable(
            &stub_bin,
            "devcontainer",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "devcontainer 0.0"
  exit 0
fi
exit 0
"#,
        );

        write_executable(
            &stub_bin,
            "docker",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Docker version 0.0"
  exit 0
fi
exit 0
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .env("PATH", prepend_path(&stub_bin))
            .args([
                "agent",
                "new",
                "agent-a",
                "--preset",
                "__does_not_exist__",
                "--no-open",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .failure();

        let worktree = agents.join("agent-a");
        assert!(
            !worktree.exists(),
            "worktree dir should be removed on failure"
        );

        let status = StdCommand::new("git")
            .current_dir(&repo)
            .args(["show-ref", "--verify", "--quiet", "refs/heads/agent-a"])
            .status()
            .unwrap();
        assert!(
            !status.success(),
            "newly-created branch should be rolled back on failure"
        );
    }

    #[test]
    fn agent_new_should_rollback_when_devcontainer_up_fails() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();

        write_executable(
            &stub_bin,
            "devcontainer",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "devcontainer 0.0"
  exit 0
fi
exit 42
"#,
        );

        write_executable(
            &stub_bin,
            "docker",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Docker version 0.0"
  exit 0
fi
exit 0
"#,
        );

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .env("PATH", prepend_path(&stub_bin))
            .args([
                "agent",
                "new",
                "agent-a",
                "--no-open",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .failure();

        let worktree = agents.join("agent-a");
        assert!(
            !worktree.exists(),
            "worktree dir should be removed on devcontainer failure"
        );

        let status = StdCommand::new("git")
            .current_dir(&repo)
            .args(["show-ref", "--verify", "--quiet", "refs/heads/agent-a"])
            .status()
            .unwrap();
        assert!(
            !status.success(),
            "newly-created branch should be rolled back on devcontainer failure"
        );
    }
}
