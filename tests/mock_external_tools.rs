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

    fn parse_worktree_from_stdout(stdout: &[u8]) -> PathBuf {
        let s = String::from_utf8_lossy(stdout);
        let line = s
            .lines()
            .find(|l| l.starts_with("Worktree: "))
            .unwrap_or_else(|| panic!("missing Worktree line in stdout:\n{s}"));
        PathBuf::from(line.trim_start_matches("Worktree: ").trim())
    }

    #[test]
    fn agent_new_opens_vscode_with_local_worktree_folder() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        let stub_bin = td.path().join("bin");
        fs::create_dir_all(&stub_bin).unwrap();
        let code_log = td.path().join("code.log");

        write_executable(
            &stub_bin,
            "code",
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "code 0.0"
  exit 0
fi
echo "ARGS:$@" >> "$PC_CODE_LOG"
exit 0
"#,
        );

        let output = Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
            .env("PC_CODE_LOG", &code_log)
            .env("PATH", prepend_path(&stub_bin))
            .args([
                "agent",
                "new",
                "agent-a",
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

        let text = fs::read_to_string(&code_log).unwrap();
        assert!(
            text.contains("ARGS:--new-window"),
            "expected VS Code to be invoked with --new-window. log: {text}"
        );
        assert!(
            text.contains(worktree.to_string_lossy().as_ref()),
            "expected VS Code to be invoked with worktree path {}. log: {text}",
            worktree.display()
        );
    }

    #[test]
    fn agent_new_rolls_back_worktree_and_branch_when_meta_write_fails() {
        let td = TempDir::new().unwrap();
        let repo = td.path().join("repo");
        init_repo(&repo);

        let agents = td.path().join("agents");
        fs::create_dir_all(&agents).unwrap();

        // Make the metadata *file path* a directory so `pc agent new` fails after creating the worktree.
        let out = StdCommand::new("git")
            .current_dir(&repo)
            .args([
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "pc/agents/agent-a.json",
            ])
            .output()
            .expect("spawn git rev-parse --git-path");
        assert!(out.status.success());
        let meta_path = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
        fs::create_dir_all(&meta_path).unwrap();

        Command::new(assert_cmd::cargo::cargo_bin!("pc"))
            .current_dir(&repo)
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
}
