use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};

const EMBEDDED_PRESETS: &[&str] = &["python-uv"];

fn pc_home_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("PC_HOME") {
        return Some(PathBuf::from(v));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".pc"))
}

fn preset_dir(preset: &str) -> Option<PathBuf> {
    Some(pc_home_dir()?.join("templates").join(preset))
}

fn read_preset_file(dir: &Path, filename: &str) -> Result<String> {
    let path = dir.join(filename);
    std::fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))
}

fn make_compose_stealth(compose: &str) -> Result<String> {
    let already_mounts_devcontainer = compose.contains("/workspaces/workspace/.devcontainer");
    let mut saw_workspace_mount = false;
    let mut inserted_devcontainer_mount = false;

    let mut out = Vec::new();
    for line in compose.lines() {
        let trimmed = line.trim_start();
        let indent_len = line.len() - trimmed.len();
        if trimmed.starts_with("- ") && trimmed.contains(":/workspaces/workspace") {
            let item = &trimmed[2..];
            if let Some(idx) = item.find(":/workspaces/workspace") {
                let rest = &item[idx..];
                let new_line = format!("{}- ${{PC_WORKSPACE_DIR}}{}", " ".repeat(indent_len), rest);
                out.push(new_line);
                saw_workspace_mount = true;

                if !already_mounts_devcontainer && !inserted_devcontainer_mount {
                    out.push(format!(
                        "{}- ${{PC_DEVCONTAINER_DIR}}:/workspaces/workspace/.devcontainer:ro",
                        " ".repeat(indent_len)
                    ));
                    inserted_devcontainer_mount = true;
                }
                continue;
            }
        }

        out.push(line.to_string());
    }

    if !saw_workspace_mount {
        bail!(
            "compose.yaml does not contain a /workspaces/workspace volume mount; cannot enable stealth mode"
        );
    }

    Ok(out.join("\n") + "\n")
}

pub fn preset_files(preset: &str) -> Result<Vec<(String, String)>> {
    if let Some(dir) = preset_dir(preset) {
        if dir.is_dir() {
            let dc = read_preset_file(&dir, "devcontainer.json")?;
            let compose = read_preset_file(&dir, "compose.yaml")?;
            let dockerfile = read_preset_file(&dir, "Dockerfile")?;
            return Ok(vec![
                ("devcontainer.json".to_string(), dc),
                ("compose.yaml".to_string(), compose),
                ("Dockerfile".to_string(), dockerfile),
            ]);
        }
    }

    match preset {
        "python-uv" => Ok(vec![
            (
                "devcontainer.json".to_string(),
                include_str!("../templates/python-uv/devcontainer.json").to_string(),
            ),
            (
                "compose.yaml".to_string(),
                include_str!("../templates/python-uv/compose.yaml").to_string(),
            ),
            (
                "Dockerfile".to_string(),
                include_str!("../templates/python-uv/Dockerfile").to_string(),
            ),
        ]),
        other => bail!("Unknown preset: {other}"),
    }
}

pub fn embedded_presets() -> &'static [&'static str] {
    EMBEDDED_PRESETS
}

pub fn install_embedded_preset(preset: &str, force: bool) -> Result<PathBuf> {
    let pc_home = pc_home_dir().ok_or_else(|| anyhow!("HOME is not set; cannot use $HOME/.pc"))?;
    let dir = pc_home.join("templates").join(preset);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    let files = match preset {
        "python-uv" => vec![
            (
                "devcontainer.json",
                include_str!("../templates/python-uv/devcontainer.json"),
            ),
            (
                "compose.yaml",
                include_str!("../templates/python-uv/compose.yaml"),
            ),
            (
                "Dockerfile",
                include_str!("../templates/python-uv/Dockerfile"),
            ),
        ],
        other => bail!("Unknown preset: {other}"),
    };

    for (name, contents) in files {
        let target = dir.join(name);
        if target.exists() && !force {
            bail!(
                "{} already exists. Use --force to overwrite.",
                target.display()
            );
        }
        std::fs::write(&target, contents)
            .with_context(|| format!("Failed to write {}", target.display()))?;
    }

    Ok(dir)
}

pub fn ensure_runtime_preset_stealth(preset: &str, force: bool) -> Result<PathBuf> {
    let pc_home = pc_home_dir().ok_or_else(|| anyhow!("HOME is not set; cannot use $HOME/.pc"))?;
    let dc_dir = pc_home
        .join("runtime")
        .join("devcontainer-presets")
        .join(preset)
        .join(".devcontainer");
    std::fs::create_dir_all(&dc_dir)
        .with_context(|| format!("Failed to create {}", dc_dir.display()))?;

    let files = preset_files(preset)?;
    for (name, contents) in files {
        let target = dc_dir.join(&name);
        if target.exists() && !force {
            continue;
        }

        let contents = if name == "compose.yaml" {
            make_compose_stealth(&contents)?
        } else {
            contents
        };

        std::fs::write(&target, contents)
            .with_context(|| format!("Failed to write {}", target.display()))?;
    }

    Ok(dc_dir)
}
