use std::fs;
use std::os::unix::fs::PermissionsExt;

use assert_cmd::Command;
use tempfile::TempDir;

#[test]
fn up_stealth_uses_config_flag_not_override_config() {
    let td = TempDir::new().unwrap();

    let ws = td.path().join("ws");
    fs::create_dir_all(&ws).unwrap();

    let bin = td.path().join("bin");
    fs::create_dir_all(&bin).unwrap();

    let args_file = td.path().join("devcontainer_args.txt");
    fs::write(&args_file, "").unwrap();

    let stub = bin.join("devcontainer");
    let script = r#"#!/bin/sh
set -eu
echo "$0 $@" >> "${PC_TEST_DEVCONTAINER_ARGS}"
exit 0
"#;
    fs::write(&stub, script).unwrap();
    fs::set_permissions(&stub, fs::Permissions::from_mode(0o755)).unwrap();

    let path = bin.display().to_string();

    Command::new(assert_cmd::cargo::cargo_bin!("pc"))
        .env("PC_HOME", td.path())
        .env("PC_TEST_DEVCONTAINER_ARGS", &args_file)
        .env("PATH", path)
        .args(["up", ws.to_str().unwrap()])
        .assert()
        .success();

    let contents = fs::read_to_string(&args_file).unwrap();
    let up_line = contents
        .lines()
        .find(|l| l.contains(" up "))
        .unwrap_or_else(|| panic!("missing devcontainer up invocation:\n{contents}"));
    assert!(
        up_line.contains(" --config "),
        "expected --config in:\n{up_line}"
    );
    assert!(
        !up_line.contains(" --override-config "),
        "did not expect --override-config in:\n{up_line}"
    );

    let expected_cfg = td
        .path()
        .join("runtime")
        .join("devcontainer-presets")
        .join("python-uv")
        .join(".devcontainer")
        .join("devcontainer.json");
    assert!(
        up_line.contains(expected_cfg.to_str().unwrap()),
        "expected config path {expected_cfg:?} in:\n{up_line}"
    );
}
