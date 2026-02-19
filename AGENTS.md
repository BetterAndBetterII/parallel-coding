# Repository Guidelines

本仓库是 `pc`（`pc-cli`）：一个 Rust CLI，用于在本机通过 `git worktree` + Dev Container 管理并行的“agent/任务环境”，并可选启动 desktop（webtop）sidecar 供浏览器调试使用。

## 项目结构

- `src/main.rs`：CLI 入口与子命令（`init` / `up` / `templates` / `agent` / `desktop-on`）。
- `src/templates.rs`：内置模板读取与 `$PC_HOME` 覆盖逻辑。
- `templates/python-uv/`：内置 devcontainer 预设（`devcontainer.json`、`compose.yaml`、`Dockerfile`）。
- `.devcontainer/`：本仓库开发用 devcontainer；其中 `.env` 可能由工具自动生成，属于预期文件。

## 构建、测试与本地开发命令

- `cargo build`：编译 CLI。
- `cargo run -- --help`：本地运行并传参调试。
- `cargo install --path .`：从当前目录安装 `pc`（详见 `README.md`）。
- `cargo test`：运行测试（若存在 `tests/` 或模块内单测）。
- `cargo fmt --all`：使用 rustfmt 格式化。
- `cargo clippy --all-targets --all-features -- -D warnings`：Clippy 静态检查（把 warning 当错误）。

## 常用工作流示例

- 初始化目录 devcontainer：`pc init . --preset python-uv`
- 启动容器：`pc up .`（缺少 `.devcontainer` 时可用 `pc up --init .`）
- 创建隔离 worktree：`pc agent new agent-a`（分支名形如 `agent/<name>`，worktree 默认在 `<repo>/../<repo-name>-agents/<agent-name>`）
- 启动桌面 sidecar：`pc agent new agent-a --desktop` 或 `pc desktop-on /path/to/dir`

## 编码风格与命名约定

- Rust 2021；保持与 `Cargo.toml` 的 `rust-version` 兼容（当前为 `1.74.1`）。
- 错误处理优先 `anyhow` + `.context(...)` 补足上下文，避免静默降级/吞错。
- 命名：模块/函数 `snake_case`；类型/trait `CamelCase`；常量 `SCREAMING_SNAKE_CASE`。

## 测试规范

- 单元测试优先写在模块内（`#[cfg(test)]`）；集成测试放在 `tests/`（例如 `tests/agent_new.rs`）。
- 对 CLI 行为，能直接调用函数就不要依赖脆弱的“纯 shell 输出断言”。

## 提交与 PR 规范

- 当前 git 历史很少（仅初始提交），尚未形成硬性约定。
- 建议采用 Conventional Commits（`feat:` / `fix:` 等），PR 描述写清“为什么改”和回归风险点。
- 涉及模板或 CLI flag 变更时，附上示例命令与预期输出/行为。

## 配置与安全提示

- 常用环境变量：`PC_HOME`、`AGENT_WORKTREE_BASE_DIR`、`WEBTOP_USERNAME`、`WEBTOP_PASSWORD`。
- 不要提交任何 secret（尤其是 `.devcontainer/.env` 或模板中的凭据）。

##（重要）错误修复与调试规则

- 若可复现：优先通过测试/最小复现定位 root cause，再做“根因修复”，不要为过测试而写应付逻辑。
- 若不易复现：允许加临时调试输出，必须带明显提示符并写 `TODO: remove debug`，然后请维护者运行并收集日志。
- 禁止滥用模糊搜索、`getattr`/`iscallable` 等“魔术运行时方法”，也不要写多分支 try-and-see 的垃圾兜底逻辑。
