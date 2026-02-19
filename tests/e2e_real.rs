#[cfg(unix)]
mod unix_only {
    use std::fs;
    use std::path::Path;
    use std::process::Command as StdCommand;

    use assert_cmd::Command;
    use serde_json::Value;
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

    fn require_tools_or_panic() {
        for (bin, args) in [
            ("git", vec!["--version"]),
            ("devcontainer", vec!["--version"]),
            ("docker", vec!["--version"]),
        ] {
            let ok = StdCommand::new(bin)
                .args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            assert!(ok, "{bin} is required for PC_E2E=1 tests");
        }

        let ok = StdCommand::new("docker")
            .args(["ps"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(
            ok,
            "docker daemon must be available (try: start docker, or fix permissions)"
        );
    }

    fn read_agent_meta(repo: &Path, agent_name: &str) -> Value {
        let rel = format!("pc/agents/{agent_name}.json");
        let output = StdCommand::new("git")
            .current_dir(repo)
            .args(["rev-parse", "--git-path", &rel])
            .output()
            .expect("spawn git rev-parse");
        assert!(output.status.success(), "git rev-parse --git-path failed");
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let text = fs::read_to_string(path).unwrap();
        serde_json::from_str(&text).unwrap()
    }

    fn docker_ps_ids(compose_project: &str, service: &str) -> Vec<String> {
        let output = StdCommand::new("docker")
            .args([
                "ps",
                "--filter",
                &format!("label=com.docker.compose.project={compose_project}"),
                "--filter",
                &format!("label=com.docker.compose.service={service}"),
                "--format",
                "{{.ID}}",
            ])
            .output()
            .expect("spawn docker ps");
        assert!(output.status.success(), "docker ps failed");
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    #[test]
    #[ignore]
    fn e2e_agent_new_starts_container_and_worktree() {
        if std::env::var("PC_E2E").ok().as_deref() != Some("1") {
            return;
        }
        require_tools_or_panic();

        let td = TempDir::new().unwrap();
        let repo = td.path().join(format!("repo-{}", std::process::id()));
        init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        let branch_name = format!("e2e-agent-{}", std::process::id());

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .args([
                "agent",
                "new",
                &branch_name,
                "--no-open",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .success();

        let worktree = agents.join(&branch_name);
        assert!(worktree.exists(), "worktree dir should exist");

        let meta = read_agent_meta(&repo, &branch_name);
        let compose_project = meta
            .get("compose_project")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();

        let ids = docker_ps_ids(&compose_project, "dev");
        assert!(
            ids.len() == 1,
            "expected 1 running dev container for {compose_project}, got: {ids:?}"
        );

        // Cleanup
        let _ = Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .args([
                "agent",
                "rm",
                &branch_name,
                "--force",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert();
    }

    #[test]
    #[ignore]
    fn e2e_agent_new_failure_rolls_back_worktree() {
        if std::env::var("PC_E2E").ok().as_deref() != Some("1") {
            return;
        }
        require_tools_or_panic();

        let td = TempDir::new().unwrap();
        let repo = td.path().join(format!("repo-{}", std::process::id()));
        init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        let pc_home = td.path().join("pc-home");
        fs::create_dir_all(&pc_home).unwrap();

        let branch_name = format!("e2e-fail-{}", std::process::id());

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_HOME", &pc_home)
            .args([
                "agent",
                "new",
                &branch_name,
                "--preset",
                "__does_not_exist__",
                "--no-open",
                "--base-dir",
                agents.to_str().unwrap(),
            ])
            .assert()
            .failure();

        let worktree = agents.join(&branch_name);
        assert!(
            !worktree.exists(),
            "worktree dir should be removed on failure"
        );

        let status = StdCommand::new("git")
            .current_dir(&repo)
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch_name}"),
            ])
            .status()
            .unwrap();
        assert!(
            !status.success(),
            "newly-created branch should be rolled back on failure"
        );
    }
}

