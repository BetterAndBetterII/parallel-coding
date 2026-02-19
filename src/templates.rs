use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::json;

const EMBEDDED_PRESETS: &[&str] = &["python-uv"];

#[derive(Debug)]
pub struct ForceRequired {
    pub target: PathBuf,
}

impl fmt::Display for ForceRequired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Target exists: {}", self.target.display())
    }
}

impl std::error::Error for ForceRequired {}

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

#[derive(Debug, Clone, Default, Serialize)]
pub struct StackSpec {
    pub python: bool,
    pub uv: bool,
    pub go: bool,
    pub node: bool,
    pub pnpm: bool,
    pub desktop: bool,
}

impl StackSpec {
    pub fn normalize(&mut self) {
        if self.uv {
            self.python = true;
        }
        if self.pnpm {
            self.node = true;
        }
    }
}

pub fn write_composed_preset(name: &str, mut spec: StackSpec, force: bool) -> Result<PathBuf> {
    spec.normalize();

    let pc_home = pc_home_dir().ok_or_else(|| anyhow!("HOME is not set; cannot use $HOME/.pc"))?;
    let dir = pc_home.join("templates").join(name);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    let files = composed_files(&spec)?;
    for (filename, contents) in files {
        let target = dir.join(&filename);
        if target.exists() && !force {
            return Err(ForceRequired { target }.into());
        }
        std::fs::write(&target, contents)
            .with_context(|| format!("Failed to write {}", target.display()))?;
    }
    Ok(dir)
}

fn composed_files(spec: &StackSpec) -> Result<Vec<(String, String)>> {
    let devcontainer = composed_devcontainer_json(spec)?;
    let compose = composed_compose_yaml(spec);
    let dockerfile = composed_dockerfile();
    Ok(vec![
        ("devcontainer.json".to_string(), devcontainer),
        ("compose.yaml".to_string(), compose),
        ("Dockerfile".to_string(), dockerfile),
    ])
}

fn composed_dockerfile() -> String {
    "FROM mcr.microsoft.com/devcontainers/base:bookworm\n".to_string()
}

fn composed_devcontainer_json(spec: &StackSpec) -> Result<String> {
    let mut features = serde_json::Map::new();
    if spec.python {
        features.insert(
            "ghcr.io/devcontainers/features/python:1".to_string(),
            json!({ "version": "3.13" }),
        );
    }
    if spec.node {
        features.insert(
            "ghcr.io/devcontainers/features/node:1".to_string(),
            json!({ "version": "22" }),
        );
    }
    if spec.go {
        features.insert(
            "ghcr.io/devcontainers/features/go:1".to_string(),
            json!({ "version": "1.22" }),
        );
    }

    let mut post_create_steps: Vec<&str> = Vec::new();
    if spec.uv {
        post_create_steps.push("command -v uv >/dev/null 2>&1 || python -m pip install --user uv");
    }
    if spec.pnpm {
        post_create_steps.push("corepack enable >/dev/null 2>&1 || true");
    }
    let post_create = if post_create_steps.is_empty() {
        None
    } else {
        Some(format!("bash -lc '{}'", post_create_steps.join(" && ")))
    };

    let mut extensions: Vec<&str> = vec!["ms-azuretools.vscode-docker"];
    if spec.python {
        extensions.push("ms-python.python");
        extensions.push("ms-python.vscode-pylance");
    }
    if spec.go {
        extensions.push("golang.go");
    }

    let mut root = serde_json::Map::new();
    root.insert("name".to_string(), json!("workspace"));
    root.insert("dockerComposeFile".to_string(), json!("compose.yaml"));
    root.insert("service".to_string(), json!("dev"));
    root.insert(
        "workspaceFolder".to_string(),
        json!("/workspaces/workspace"),
    );
    root.insert("remoteUser".to_string(), json!("vscode"));
    root.insert("updateRemoteUserUID".to_string(), json!(true));
    if !features.is_empty() {
        root.insert("features".to_string(), serde_json::Value::Object(features));
    }
    if let Some(cmd) = post_create {
        root.insert("postCreateCommand".to_string(), json!(cmd));
    }
    root.insert(
        "customizations".to_string(),
        json!({
            "vscode": {
                "extensions": extensions,
            }
        }),
    );

    let v = serde_json::Value::Object(root);
    Ok(serde_json::to_string_pretty(&v)? + "\n")
}

fn composed_compose_yaml(spec: &StackSpec) -> String {
    let mut volumes = Vec::new();
    let mut volume_defs = Vec::new();

    if spec.python {
        volumes.push("      - uv_cache:/home/vscode/.cache/uv".to_string());
        volumes.push("      - pip_cache:/home/vscode/.cache/pip".to_string());
        volume_defs.push("  uv_cache:".to_string());
        volume_defs
            .push("    name: ${DEVCONTAINER_CACHE_PREFIX:-devcontainer}-uv-cache".to_string());
        volume_defs.push("  pip_cache:".to_string());
        volume_defs
            .push("    name: ${DEVCONTAINER_CACHE_PREFIX:-devcontainer}-pip-cache".to_string());
    }

    if spec.node {
        volumes.push("      - pnpm_home:/home/vscode/.local/share/pnpm".to_string());
        volumes.push("      - npm_cache:/home/vscode/.npm".to_string());
        volume_defs.push("  pnpm_home:".to_string());
        volume_defs
            .push("    name: ${DEVCONTAINER_CACHE_PREFIX:-devcontainer}-pnpm-home".to_string());
        volume_defs.push("  npm_cache:".to_string());
        volume_defs
            .push("    name: ${DEVCONTAINER_CACHE_PREFIX:-devcontainer}-npm-cache".to_string());
    }

    if spec.go {
        volumes.push("      - go_mod_cache:/home/vscode/go/pkg/mod".to_string());
        volumes.push("      - go_build_cache:/home/vscode/.cache/go-build".to_string());
        volume_defs.push("  go_mod_cache:".to_string());
        volume_defs
            .push("    name: ${DEVCONTAINER_CACHE_PREFIX:-devcontainer}-go-mod-cache".to_string());
        volume_defs.push("  go_build_cache:".to_string());
        volume_defs.push(
            "    name: ${DEVCONTAINER_CACHE_PREFIX:-devcontainer}-go-build-cache".to_string(),
        );
    }

    let mut envs = Vec::new();
    if spec.python {
        envs.push("      UV_CACHE_DIR: /home/vscode/.cache/uv".to_string());
        envs.push("      PIP_CACHE_DIR: /home/vscode/.cache/pip".to_string());
        envs.push("      PYTHONDONTWRITEBYTECODE: \"1\"".to_string());
        envs.push("      PYTHONUNBUFFERED: \"1\"".to_string());
    }
    if spec.node {
        envs.push("      NPM_CONFIG_CACHE: /home/vscode/.npm".to_string());
        envs.push("      PNPM_HOME: /home/vscode/.local/share/pnpm".to_string());
        envs.push("      PATH: /home/vscode/.local/share/pnpm:${PATH}".to_string());
    }
    if spec.go {
        envs.push("      GOMODCACHE: /home/vscode/go/pkg/mod".to_string());
        envs.push("      GOCACHE: /home/vscode/.cache/go-build".to_string());
    }

    let desktop_service = if spec.desktop {
        r#"

  desktop:
    image: lscr.io/linuxserver/webtop:ubuntu-kde
    profiles: ["desktop"]
    shm_size: "1gb"
    environment:
      PUID: "1000"
      PGID: "1000"
      TZ: ${TZ:-Etc/UTC}
      USERNAME: ${WEBTOP_USERNAME:-vscode}
      PASSWORD: ${WEBTOP_PASSWORD:-}
    volumes:
      - desktop_home:/config
    ports:
      - "127.0.0.1::3000"
    restart: unless-stopped
"#
        .to_string()
    } else {
        "".to_string()
    };

    let mut out = String::new();
    out.push_str("services:\n");
    out.push_str("  dev:\n");
    out.push_str("    build:\n");
    out.push_str("      context: .\n");
    out.push_str("      dockerfile: Dockerfile\n");
    out.push_str("    volumes:\n");
    out.push_str("      - ..:/workspaces/workspace:cached\n");
    for v in volumes {
        out.push_str(&v);
        out.push('\n');
    }
    if !envs.is_empty() {
        out.push_str("    environment:\n");
        for e in envs {
            out.push_str(&e);
            out.push('\n');
        }
    }
    out.push_str("    command: sleep infinity\n");
    out.push_str(&desktop_service);
    if spec.desktop || !volume_defs.is_empty() {
        out.push_str("\nvolumes:\n");
        if spec.desktop {
            out.push_str("  desktop_home: {}\n");
        }
        for line in volume_defs {
            out.push_str(&line);
            out.push('\n');
        }
    } else {
        out.push_str("\nvolumes: {}\n");
    }
    out
}

fn read_preset_file(dir: &Path, filename: &str) -> Result<String> {
    let path = dir.join(filename);
    std::fs::read_to_string(&path).with_context(|| format!("Failed to read {}", path.display()))
}

fn sanitize_image_tag(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' || ch == '-' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "default".to_string()
    } else {
        out
    }
}

fn make_compose_stealth(compose: &str, default_image: &str) -> Result<String> {
    let already_mounts_devcontainer = compose.contains("/workspaces/workspace/.devcontainer");
    let mut saw_workspace_mount = false;
    let mut inserted_devcontainer_mount = false;

    let default_image = sanitize_image_tag(default_image);
    let image_line = format!("    image: ${{DEVCONTAINER_IMAGE:-pc-devcontainer:{default_image}}}");

    let mut in_dev_service = false;
    let mut skipping_build_block = false;
    let mut out = Vec::new();
    for line in compose.lines() {
        let trimmed = line.trim_start();
        let indent_len = line.len() - trimmed.len();

        if indent_len == 2 && trimmed == "dev:" {
            in_dev_service = true;
        } else if indent_len == 2 && trimmed.ends_with(':') && trimmed != "dev:" {
            in_dev_service = false;
        }

        if in_dev_service && skipping_build_block {
            if indent_len > 4 {
                continue;
            }
            skipping_build_block = false;
        }

        if in_dev_service && indent_len == 4 && trimmed == "build:" {
            out.push(image_line.clone());
            skipping_build_block = true;
            continue;
        }

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
            return Err(ForceRequired { target }.into());
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
            make_compose_stealth(&contents, preset)?
        } else {
            contents
        };

        std::fs::write(&target, contents)
            .with_context(|| format!("Failed to write {}", target.display()))?;
    }

    Ok(dc_dir)
}
