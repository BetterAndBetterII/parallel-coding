#[cfg(unix)]
#[path = "common/mod.rs"]
mod common;

#[cfg(unix)]
mod unix_only {
    use std::fs;
    use std::path::Path;
    use std::process::Command as StdCommand;

    use assert_cmd::Command;
    use predicates::str::contains;
    use tempfile::TempDir;

    use super::common;

    fn git_show_ref(repo: &Path, reference: &str) -> bool {
        StdCommand::new("git")
            .current_dir(repo)
            .args(["show-ref", "--verify", "--quiet", reference])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn git_path(repo: &Path, rel: &str) -> String {
        let out = StdCommand::new("git")
            .current_dir(repo)
            .args(["rev-parse", "--path-format=absolute", "--git-path", rel])
            .output()
            .expect("spawn git rev-parse --git-path");
        assert!(out.status.success());
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn agent_rm_finds_worktree_by_branch_name_and_removes_only_worktree() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        common::init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .args([
                "agent",
                "new",
                "feat/a",
                "--no-up",
                "--no-open",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .success();

        assert!(agents.join("feat_a").exists());
        assert!(git_show_ref(&repo, "refs/heads/feat/a"));

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();
        common::write_executable(
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
            .env("PATH", common::prepend_path(&stub_bin))
            .args([
                "agent",
                "rm",
                "feat/a",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .success();

        assert!(
            !agents.join("feat_a").exists(),
            "worktree should be removed"
        );
        assert!(
            git_show_ref(&repo, "refs/heads/feat/a"),
            "branch should remain after rm"
        );
    }

    #[test]
    fn agent_rm_reads_old_meta_without_branch_name_field() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        common::init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .args([
                "agent",
                "new",
                "feat/a",
                "--no-up",
                "--no-open",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .success();

        let meta_path = git_path(&repo, "pc/agents/feat_a.json");
        fs::write(
            &meta_path,
            r#"{
  "preset": "python-uv",
  "compose_project": "agent_feat_a",
  "cache_prefix": "repo"
}
"#,
        )
        .unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();
        common::write_executable(
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
            .env("PATH", common::prepend_path(&stub_bin))
            .args([
                "agent",
                "rm",
                "feat/a",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .success();

        assert!(!agents.join("feat_a").exists());

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .env("PATH", common::prepend_path(&stub_bin))
            .args([
                "agent",
                "rm",
                "feat/a",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .failure()
            .stderr(contains("Agent worktree not found"));
    }
}
