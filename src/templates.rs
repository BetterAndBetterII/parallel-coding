use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};

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

static EMBEDDED_TEMPLATES_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/templates");

fn pc_home_dir() -> Option<PathBuf> {
    if let Some(v) = std::env::var_os("PC_HOME") {
        return Some(PathBuf::from(v));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".pc"))
}

fn templates_root_dir() -> Option<PathBuf> {
    Some(pc_home_dir()?.join("templates"))
}

fn user_templates_dir(preset: &str) -> Option<PathBuf> {
    Some(templates_root_dir()?.join(preset))
}

fn user_components_root_dir() -> Option<PathBuf> {
    Some(templates_root_dir()?.join(".components"))
}

fn user_profiles_root_dir() -> Option<PathBuf> {
    Some(templates_root_dir()?.join(".profiles"))
}

#[derive(Debug, Clone)]
pub struct TemplateFile {
    pub rel_path: PathBuf,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComposeSpec {
    pub components: Vec<String>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComponentParam {
    pub key: String,
    pub prompt: String,
    pub default: String,
    #[serde(default)]
    pub choices: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComponentManifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub depends: Vec<String>,
    #[serde(default)]
    pub conflicts: Vec<String>,
    #[serde(default)]
    pub params: Vec<ComponentParam>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileManifest {
    #[serde(default)]
    pub components: Vec<String>,
    #[serde(default)]
    pub params: BTreeMap<String, String>,
}

pub fn embedded_component_manifests() -> Result<Vec<ComponentManifest>> {
    let mut out = Vec::new();
    let dir = EMBEDDED_TEMPLATES_DIR
        .get_dir("components")
        .ok_or_else(|| anyhow!("Embedded templates missing templates/components/"))?;
    for (id, bytes) in embedded_find_component_tomls(dir, "components") {
        let s = std::str::from_utf8(&bytes)
            .with_context(|| format!("Embedded component.toml not UTF-8 for {id}"))?;
        let m: ComponentManifest = toml::from_str(s)
            .with_context(|| format!("Failed to parse component.toml for {id}"))?;
        out.push(m);
    }
    Ok(out)
}

pub fn embedded_profile_names() -> Vec<String> {
    let Some(dir) = EMBEDDED_TEMPLATES_DIR.get_dir("profiles") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for d in dir.dirs() {
        let Some(name) = d.path().file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        // Note: include_dir's lookups are relative to the included root.
        if d.get_file(d.path().join("profile.toml")).is_some() {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}

pub fn component_manifests() -> Result<Vec<ComponentManifest>> {
    let mut merged: BTreeMap<String, ComponentManifest> = BTreeMap::new();
    for m in embedded_component_manifests()? {
        merged.insert(m.id.clone(), m);
    }

    if let Some(root) = user_components_root_dir() {
        if root.is_dir() {
            for path in find_component_tomls_on_fs(&root)? {
                let s = std::fs::read_to_string(&path)
                    .with_context(|| format!("Failed to read {}", path.display()))?;
                let m: ComponentManifest = toml::from_str(&s)
                    .with_context(|| format!("Failed to parse {}", path.display()))?;
                merged.insert(m.id.clone(), m);
            }
        }
    }

    Ok(merged.into_values().collect())
}

fn find_component_tomls_on_fs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for p in walk_dir_files(root)? {
        if p.file_name().and_then(|s| s.to_str()) == Some("component.toml") {
            out.push(p);
        }
    }
    Ok(out)
}

pub fn component_param_defs(components: &[String]) -> Result<Vec<ComponentParam>> {
    let resolved = resolve_components(components)?;
    let mut defs: BTreeMap<String, ComponentParam> = BTreeMap::new();
    for id in resolved {
        let c = load_component(&id)?;
        for p in c.manifest.params {
            defs.entry(p.key.clone()).or_insert(p);
        }
    }
    Ok(defs.into_values().collect())
}

fn embedded_find_component_tomls(dir: &include_dir::Dir<'_>, base: &str) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    for f in dir.files() {
        if f.path().file_name().and_then(|s| s.to_str()) == Some("component.toml") {
            let rel = f.path().to_string_lossy().to_string();
            let id = rel
                .strip_prefix(&format!("{base}/"))
                .unwrap_or(&rel)
                .trim_end_matches("/component.toml")
                .to_string();
            out.push((id, f.contents().to_vec()));
        }
    }
    for d in dir.dirs() {
        out.extend(embedded_find_component_tomls(d, base));
    }
    out
}

pub fn write_composed_template(name: &str, spec: ComposeSpec, force: bool) -> Result<PathBuf> {
    validate_template_name(name)?;

    let root =
        templates_root_dir().ok_or_else(|| anyhow!("HOME is not set; cannot use $HOME/.pc"))?;
    std::fs::create_dir_all(&root)
        .with_context(|| format!("Failed to create {}", root.display()))?;
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    let files = render_from_components(&spec.components, &spec.params)?;
    write_template_dir(&dir, &files, force)?;
    Ok(dir)
}

fn write_template_dir(dir: &Path, files: &[TemplateFile], force: bool) -> Result<()> {
    for f in files {
        let target = dir.join(&f.rel_path);
        if target.exists() && !force {
            return Err(ForceRequired {
                target: target.clone(),
            }
            .into());
        }
    }
    for f in files {
        let target = dir.join(&f.rel_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        std::fs::write(&target, &f.bytes)
            .with_context(|| format!("Failed to write {}", target.display()))?;
    }
    Ok(())
}

fn validate_template_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("template name cannot be empty");
    }
    if name == ".components" || name == ".profiles" || name == "runtime" {
        bail!("template name {name} is reserved");
    }
    if name.contains('/') || name.contains('\\') {
        bail!("template name must not contain path separators");
    }
    Ok(())
}

pub fn render_from_components(
    components: &[String],
    params: &BTreeMap<String, String>,
) -> Result<Vec<TemplateFile>> {
    let mut resolved = resolve_components(components)?;
    ensure_base_component(&mut resolved);

    let mut effective_params = params.clone();
    for id in &resolved {
        let c = load_component(id)?;
        for p in c.manifest.params {
            effective_params.entry(p.key).or_insert(p.default);
        }
    }

    let mut all_files: Vec<TemplateFile> = Vec::new();
    let mut docker_parts: Vec<(String, String)> = Vec::new();
    let mut devcontainer_fragments: Vec<(String, serde_json::Value)> = Vec::new();
    let mut compose_fragments: Vec<(String, serde_yaml::Value)> = Vec::new();

    for id in &resolved {
        let c = load_component(id)?;

        if let Some(s) = c.devcontainer_json {
            let s = apply_params_str(&s, &effective_params);
            let v: serde_json::Value = serde_json::from_str(&s)
                .with_context(|| format!("Failed to parse devcontainer.json fragment for {id}"))?;
            devcontainer_fragments.push((id.clone(), v));
        }
        if let Some(s) = c.compose_yaml {
            let s = apply_params_str(&s, &effective_params);
            let v: serde_yaml::Value = serde_yaml::from_str(&s)
                .with_context(|| format!("Failed to parse compose.yaml fragment for {id}"))?;
            compose_fragments.push((id.clone(), v));
        }
        if let Some(s) = c.dockerfile_part {
            let s = apply_params_str(&s, &effective_params);
            docker_parts.push((id.clone(), s));
        }
        for mut f in c.extra_files {
            if let Ok(s) = std::str::from_utf8(&f.bytes) {
                f.bytes = apply_params_str(s, &effective_params).into_bytes();
            }
            all_files.push(f);
        }
    }

    let devcontainer = merge_json_fragments(&devcontainer_fragments)?;
    let compose = merge_yaml_fragments(&compose_fragments)?;
    let dockerfile = render_dockerfile(&docker_parts)?;

    all_files.push(TemplateFile {
        rel_path: PathBuf::from("devcontainer.json"),
        bytes: (serde_json::to_string_pretty(&devcontainer)? + "\n").into_bytes(),
    });
    all_files.push(TemplateFile {
        rel_path: PathBuf::from("compose.yaml"),
        bytes: (serde_yaml::to_string(&compose)?).into_bytes(),
    });
    all_files.push(TemplateFile {
        rel_path: PathBuf::from("Dockerfile"),
        bytes: dockerfile.into_bytes(),
    });

    Ok(stable_dedup_files(all_files))
}

fn ensure_base_component(components: &mut Vec<String>) {
    if components.iter().any(|c| c == "base/devcontainer") {
        return;
    }
    components.insert(0, "base/devcontainer".to_string());
}

fn stable_dedup_files(files: Vec<TemplateFile>) -> Vec<TemplateFile> {
    let mut seen = BTreeSet::<String>::new();
    let mut out = Vec::new();
    for f in files {
        let k = f.rel_path.to_string_lossy().to_string();
        if seen.insert(k) {
            out.push(f);
        }
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
}

fn apply_params_str(s: &str, params: &BTreeMap<String, String>) -> String {
    let mut out = s.to_string();
    for (k, v) in params {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

fn merge_json_fragments(frags: &[(String, serde_json::Value)]) -> Result<serde_json::Value> {
    let mut root = serde_json::Value::Object(serde_json::Map::new());
    for (id, v) in frags {
        merge_json_value(&mut root, v, id, "$")?;
    }
    Ok(root)
}

fn merge_json_value(
    dst: &mut serde_json::Value,
    src: &serde_json::Value,
    src_id: &str,
    path: &str,
) -> Result<()> {
    match (dst, src) {
        (serde_json::Value::Object(d), serde_json::Value::Object(s)) => {
            for (k, sv) in s {
                let sub_path = format!("{path}.{k}");
                match d.get_mut(k) {
                    None => {
                        d.insert(k.clone(), sv.clone());
                    }
                    Some(dv) => {
                        if dv == sv {
                            continue;
                        }
                        if dv.is_object() && sv.is_object() {
                            merge_json_value(dv, sv, src_id, &sub_path)?;
                        } else if dv.is_array() && sv.is_array() {
                            merge_json_value(dv, sv, src_id, &sub_path)?;
                        } else {
                            bail!("Conflict at {sub_path} while merging component {src_id}");
                        }
                    }
                }
            }
            Ok(())
        }
        (serde_json::Value::Array(d), serde_json::Value::Array(s)) => {
            for item in s {
                d.push(item.clone());
            }
            // Best-effort de-dup scalar arrays.
            let all_scalar = d.iter().all(|v| matches!(v, serde_json::Value::String(_)))
                || d.iter().all(|v| matches!(v, serde_json::Value::Number(_)))
                || d.iter().all(|v| matches!(v, serde_json::Value::Bool(_)));
            if all_scalar {
                let mut seen = BTreeSet::new();
                d.retain(|v| seen.insert(v.to_string()));
            }
            Ok(())
        }
        (d, s) => {
            if d == s {
                Ok(())
            } else {
                bail!("Type conflict at {path} while merging component {src_id}");
            }
        }
    }
}

fn merge_yaml_fragments(frags: &[(String, serde_yaml::Value)]) -> Result<serde_yaml::Value> {
    let mut root = serde_yaml::Value::Mapping(serde_yaml::Mapping::new());
    for (id, v) in frags {
        merge_yaml_value(&mut root, v, id, "$")?;
    }
    Ok(root)
}

fn merge_yaml_value(
    dst: &mut serde_yaml::Value,
    src: &serde_yaml::Value,
    src_id: &str,
    path: &str,
) -> Result<()> {
    match (dst, src) {
        (serde_yaml::Value::Mapping(d), serde_yaml::Value::Mapping(s)) => {
            for (k, sv) in s {
                let key_str = match k {
                    serde_yaml::Value::String(x) => x.clone(),
                    _ => format!("{k:?}"),
                };
                let sub_path = format!("{path}.{key_str}");
                match d.get_mut(k) {
                    None => {
                        d.insert(k.clone(), sv.clone());
                    }
                    Some(dv) => {
                        if dv == sv {
                            continue;
                        }
                        if dv.is_mapping() && sv.is_mapping() {
                            merge_yaml_value(dv, sv, src_id, &sub_path)?;
                        } else if dv.is_sequence() && sv.is_sequence() {
                            merge_yaml_value(dv, sv, src_id, &sub_path)?;
                        } else {
                            bail!("Conflict at {sub_path} while merging component {src_id}");
                        }
                    }
                }
            }
            Ok(())
        }
        (serde_yaml::Value::Sequence(d), serde_yaml::Value::Sequence(s)) => {
            for item in s {
                d.push(item.clone());
            }
            let all_str = d.iter().all(|v| matches!(v, serde_yaml::Value::String(_)));
            if all_str {
                let mut seen = BTreeSet::new();
                d.retain(|v| seen.insert(format!("{v:?}")));
            }
            Ok(())
        }
        (d, s) => {
            if d == s {
                Ok(())
            } else {
                bail!("Type conflict at {path} while merging component {src_id}");
            }
        }
    }
}

fn render_dockerfile(parts: &[(String, String)]) -> Result<String> {
    if parts.is_empty() {
        return Ok("FROM mcr.microsoft.com/devcontainers/base:bookworm\n".to_string());
    }

    let mut out = String::new();
    // Keep the first Dockerfile part at the top so the final Dockerfile starts with `FROM ...`,
    // matching common expectations and existing tests.
    let mut iter = parts.iter();
    if let Some((first_id, first_part)) = iter.next() {
        out.push_str(first_part);
        if !first_part.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
        out.push_str(&format!("# pc:component {first_id} end\n\n"));
    }

    for (id, part) in iter {
        out.push_str(&format!("# pc:component {id} begin\n"));
        out.push_str(part);
        if !part.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("# pc:component {id} end\n\n"));
    }
    Ok(out)
}

#[derive(Debug)]
struct LoadedComponent {
    manifest: ComponentManifest,
    devcontainer_json: Option<String>,
    compose_yaml: Option<String>,
    dockerfile_part: Option<String>,
    extra_files: Vec<TemplateFile>,
}

fn load_component(id: &str) -> Result<LoadedComponent> {
    if id.is_empty() {
        bail!("component id cannot be empty");
    }
    if id.contains("..") {
        bail!("invalid component id: {id}");
    }

    if let Some(root) = user_components_root_dir() {
        let p = root.join(id);
        if p.is_dir() {
            return load_component_from_fs(&p);
        }
    }

    let p = format!("components/{id}");
    let dir = EMBEDDED_TEMPLATES_DIR
        .get_dir(&p)
        .ok_or_else(|| anyhow!("Unknown component: {id}"))?;
    load_component_from_embedded(dir)
}

fn load_component_from_fs(dir: &Path) -> Result<LoadedComponent> {
    let manifest_path = dir.join("component.toml");
    let manifest_s = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
    let manifest: ComponentManifest = toml::from_str(&manifest_s)
        .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;

    let devcontainer_json = read_opt_text(dir.join("devcontainer.json"))?;
    let compose_yaml = read_opt_text(dir.join("compose.yaml"))?;
    let dockerfile_part = read_opt_text(dir.join("Dockerfile.part"))?;
    let extra_files = read_opt_files_tree(&dir.join("files"))?;

    Ok(LoadedComponent {
        manifest,
        devcontainer_json,
        compose_yaml,
        dockerfile_part,
        extra_files,
    })
}

fn read_opt_text(path: PathBuf) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(std::fs::read_to_string(&path).with_context(|| {
        format!("Failed to read {}", path.display())
    })?))
}

fn read_opt_files_tree(root: &Path) -> Result<Vec<TemplateFile>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in walk_dir_files(root)? {
        let bytes =
            std::fs::read(&entry).with_context(|| format!("Failed to read {}", entry.display()))?;
        let rel = entry
            .strip_prefix(root)
            .with_context(|| format!("Failed to strip prefix {}", root.display()))?;
        out.push(TemplateFile {
            rel_path: rel.to_path_buf(),
            bytes,
        });
    }
    Ok(out)
}

fn walk_dir_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for entry in
        std::fs::read_dir(root).with_context(|| format!("Failed to read {}", root.display()))?
    {
        let entry =
            entry.with_context(|| format!("Failed to read entry under {}", root.display()))?;
        let path = entry.path();
        let meta = entry
            .metadata()
            .with_context(|| format!("Failed to stat {}", path.display()))?;
        if meta.is_dir() {
            out.extend(walk_dir_files(&path)?);
        } else if meta.is_file() {
            out.push(path);
        }
    }
    Ok(out)
}

fn load_component_from_embedded(dir: &include_dir::Dir<'_>) -> Result<LoadedComponent> {
    let manifest_f = dir
        .get_file(dir.path().join("component.toml"))
        .ok_or_else(|| {
            anyhow!(
                "Embedded component missing component.toml: {}",
                dir.path().display()
            )
        })?;
    let manifest_s =
        std::str::from_utf8(manifest_f.contents()).context("Embedded component.toml not UTF-8")?;
    let manifest: ComponentManifest =
        toml::from_str(manifest_s).context("Failed to parse embedded component.toml")?;

    let devcontainer_json = dir
        .get_file(dir.path().join("devcontainer.json"))
        .map(|f| {
            std::str::from_utf8(f.contents())
                .context("Embedded devcontainer.json not UTF-8")
                .map(|s| s.to_string())
        })
        .transpose()?;
    let compose_yaml = dir
        .get_file(dir.path().join("compose.yaml"))
        .map(|f| {
            std::str::from_utf8(f.contents())
                .context("Embedded compose.yaml not UTF-8")
                .map(|s| s.to_string())
        })
        .transpose()?;
    let dockerfile_part = dir
        .get_file(dir.path().join("Dockerfile.part"))
        .map(|f| {
            std::str::from_utf8(f.contents())
                .context("Embedded Dockerfile.part not UTF-8")
                .map(|s| s.to_string())
        })
        .transpose()?;

    let mut extra_files = Vec::new();
    if let Some(files_dir) = dir.get_dir(dir.path().join("files")) {
        extra_files.extend(read_embedded_files_tree(files_dir, Path::new(""))?);
    }

    Ok(LoadedComponent {
        manifest,
        devcontainer_json,
        compose_yaml,
        dockerfile_part,
        extra_files,
    })
}

fn read_embedded_files_tree(dir: &include_dir::Dir<'_>, rel: &Path) -> Result<Vec<TemplateFile>> {
    let mut out = Vec::new();
    for f in dir.files() {
        let name = f
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid embedded filename under {}", dir.path().display()))?;
        let p = rel.join(name);
        out.push(TemplateFile {
            rel_path: p,
            bytes: f.contents().to_vec(),
        });
    }
    for d in dir.dirs() {
        let name = d
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid embedded dirname under {}", dir.path().display()))?;
        out.extend(read_embedded_files_tree(d, &rel.join(name))?);
    }
    Ok(out)
}

fn resolve_components(requested: &[String]) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();

    let mut initial = Vec::new();
    for c in requested {
        let mapped = map_legacy_component_name(c);
        initial.push(mapped.to_string());
    }

    for id in initial {
        dfs_component(&id, &mut out, &mut visiting, &mut visited)?;
    }
    check_conflicts(&out)?;
    Ok(out)
}

fn dfs_component(
    id: &str,
    out: &mut Vec<String>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> Result<()> {
    if visited.contains(id) {
        return Ok(());
    }
    if !visiting.insert(id.to_string()) {
        bail!("Dependency cycle detected at component {id}");
    }
    let c = load_component(id)?;
    for dep in &c.manifest.depends {
        dfs_component(dep, out, visiting, visited)?;
    }
    visiting.remove(id);
    visited.insert(id.to_string());
    out.push(id.to_string());
    Ok(())
}

fn check_conflicts(resolved: &[String]) -> Result<()> {
    let mut present = BTreeSet::new();
    for id in resolved {
        present.insert(id.clone());
    }
    for id in resolved {
        let c = load_component(id)?;
        for conflict in &c.manifest.conflicts {
            if present.contains(conflict) {
                bail!("Component conflict: {id} conflicts with {conflict}");
            }
        }
    }
    Ok(())
}

fn map_legacy_component_name(s: &str) -> &str {
    match s {
        "python" => "lang/python",
        "uv" => "tool/python/uv",
        "go" => "lang/go",
        "node" => "lang/node",
        "pnpm" => "tool/node/pnpm",
        "desktop" => "extra/desktop",
        other => other,
    }
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

pub fn preset_files(preset: &str) -> Result<Vec<TemplateFile>> {
    if let Some(dir) = user_templates_dir(preset) {
        if dir.is_dir() {
            ensure_fs_template_dir_complete(&dir)?;
            return read_fs_template_dir(&dir);
        }
    }

    if let Some(root) = user_profiles_root_dir() {
        let p = root.join(preset).join("profile.toml");
        if p.exists() {
            let profile = read_profile_from_fs(&p)?;
            return render_from_components(&profile.components, &profile.params);
        }
    }

    if let Some(dir) = EMBEDDED_TEMPLATES_DIR.get_dir(preset) {
        if dir.get_file("devcontainer.json").is_some() {
            return read_embedded_template_dir(dir);
        }
    }

    if let Some(profile) = read_profile_from_embedded(preset)? {
        return render_from_components(&profile.components, &profile.params);
    }

    bail!("Unknown preset/profile: {preset}")
}

fn ensure_fs_template_dir_complete(dir: &Path) -> Result<()> {
    for name in ["devcontainer.json", "compose.yaml", "Dockerfile"] {
        let path = dir.join(name);
        // Use read_to_string to preserve legacy error wording ("Failed to read ...") which
        // callers/tests rely on for incomplete $PC_HOME overrides.
        let _ = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
    }
    Ok(())
}

fn read_fs_template_dir(dir: &Path) -> Result<Vec<TemplateFile>> {
    let mut out = Vec::new();
    for f in walk_dir_files(dir)? {
        let rel = f
            .strip_prefix(dir)
            .with_context(|| format!("Failed to strip prefix {}", dir.display()))?
            .to_path_buf();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let bytes = std::fs::read(&f).with_context(|| format!("Failed to read {}", f.display()))?;
        out.push(TemplateFile {
            rel_path: rel,
            bytes,
        });
    }
    Ok(out)
}

fn read_embedded_template_dir(dir: &include_dir::Dir<'_>) -> Result<Vec<TemplateFile>> {
    let mut out = Vec::new();
    for f in dir.files() {
        let name = f
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid embedded filename under {}", dir.path().display()))?;
        out.push(TemplateFile {
            rel_path: PathBuf::from(name),
            bytes: f.contents().to_vec(),
        });
    }
    for d in dir.dirs() {
        let name = d
            .path()
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("Invalid embedded dirname under {}", dir.path().display()))?;
        for f in read_embedded_template_dir(d)? {
            out.push(TemplateFile {
                rel_path: PathBuf::from(name).join(f.rel_path),
                bytes: f.bytes,
            });
        }
    }
    Ok(out)
}

fn read_profile_from_fs(path: &Path) -> Result<ProfileManifest> {
    let s = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    toml::from_str(&s).with_context(|| format!("Failed to parse {}", path.display()))
}

fn read_profile_from_embedded(name: &str) -> Result<Option<ProfileManifest>> {
    let p = format!("profiles/{name}/profile.toml");
    let Some(f) = EMBEDDED_TEMPLATES_DIR.get_file(&p) else {
        return Ok(None);
    };
    let s = std::str::from_utf8(f.contents()).with_context(|| format!("Embedded {p} not UTF-8"))?;
    let m: ProfileManifest =
        toml::from_str(s).with_context(|| format!("Failed to parse embedded {p}"))?;
    Ok(Some(m))
}

pub fn embedded_presets() -> Vec<String> {
    let mut out = Vec::new();
    for d in EMBEDDED_TEMPLATES_DIR.dirs() {
        let Some(name) = d.path().file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name == "components" || name == "profiles" {
            continue;
        }
        if d.get_file("devcontainer.json").is_some() {
            out.push(name.to_string());
        }
    }
    out.sort();
    out
}

pub fn install_embedded_preset(preset: &str, force: bool) -> Result<PathBuf> {
    let root =
        templates_root_dir().ok_or_else(|| anyhow!("HOME is not set; cannot use $HOME/.pc"))?;
    let dir = root.join(preset);
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    if let Some(embedded) = EMBEDDED_TEMPLATES_DIR.get_dir(preset) {
        if embedded.get_file("devcontainer.json").is_some() {
            let files = read_embedded_template_dir(embedded)?;
            write_template_dir(&dir, &files, force)?;
            return Ok(dir);
        }
    }

    if let Some(profile) = read_profile_from_embedded(preset)? {
        let files = render_from_components(&profile.components, &profile.params)?;
        write_template_dir(&dir, &files, force)?;
        return Ok(dir);
    }

    bail!("Unknown embedded preset/profile: {preset}")
}

pub fn install_embedded_components(force: bool) -> Result<PathBuf> {
    let root =
        templates_root_dir().ok_or_else(|| anyhow!("HOME is not set; cannot use $HOME/.pc"))?;
    let dir = root.join(".components");
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    let Some(src) = EMBEDDED_TEMPLATES_DIR.get_dir("components") else {
        bail!("Embedded templates missing templates/components/");
    };
    let files = read_embedded_template_dir(src)?;
    write_template_dir(&dir, &files, force)?;
    Ok(dir)
}

pub fn install_embedded_profiles(force: bool) -> Result<PathBuf> {
    let root =
        templates_root_dir().ok_or_else(|| anyhow!("HOME is not set; cannot use $HOME/.pc"))?;
    let dir = root.join(".profiles");
    std::fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    let Some(src) = EMBEDDED_TEMPLATES_DIR.get_dir("profiles") else {
        bail!("Embedded templates missing templates/profiles/");
    };
    let files = read_embedded_template_dir(src)?;
    write_template_dir(&dir, &files, force)?;
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
    for f in files {
        let target = dc_dir.join(&f.rel_path);
        if target.exists() && !force {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        let bytes = if f.rel_path == PathBuf::from("compose.yaml") {
            let s = std::str::from_utf8(&f.bytes).context("compose.yaml is not UTF-8")?;
            make_compose_stealth(s, preset)?.into_bytes()
        } else {
            f.bytes
        };
        std::fs::write(&target, bytes)
            .with_context(|| format!("Failed to write {}", target.display()))?;
    }

    Ok(dc_dir)
}
