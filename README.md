# pc-cli (`pc`)

[![CI](https://github.com/BetterAndBetterII/parallel-coding/actions/workflows/ci.yml/badge.svg)](https://github.com/BetterAndBetterII/parallel-coding/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/BetterAndBetterII/parallel-coding/branch/main/graph/badge.svg)](https://codecov.io/gh/BetterAndBetterII/parallel-coding)

一个可安装的 Rust CLI，用于在本机用 **git worktree + Dev Container** 管理“并行 Agent/任务环境”，并可选启动每个任务独立的 **webtop 桌面容器**用于浏览器调试。

## 安装

### Linux (Debian/Ubuntu) 一键安装（从 GitHub Release 下载二进制）

当前提供：`x86_64-unknown-linux-gnu`。

```bash
curl -fsSL https://github.com/BetterAndBetterII/parallel-coding/releases/latest/download/pc-x86_64-unknown-linux-gnu -o /tmp/pc && sudo install -m 755 /tmp/pc /usr/local/bin/pc && rm /tmp/pc
```

安装后可在任意目录使用：

```bash
pc --help
```

### 从源码安装（需要 Rust / cargo）

在仓库目录内：

```bash
cargo install --path .
```

安装后可在任意目录使用：`pc --help`

## 依赖

- `git`
- `devcontainer` CLI（`@devcontainers/cli`）
- `docker`（可选：仅在 `desktop-on` 打印 URL 时需要）
- `code`（可选：`agent new` 自动打开 VS Code 新窗口）

本仓库仍保留了等价的 shell 脚本在 `scripts/` 作为参考。

## 用法（常用）

### 0) （可选）把默认模板安装到 `$HOME/.pc` 方便自定义

默认内置一个 `python-uv` 模板；如果你想在全局复用并可修改它：

```bash
pc templates init
```

会写入：`$HOME/.pc/templates/python-uv/{devcontainer.json,compose.yaml,Dockerfile}`（可用环境变量 `PC_HOME` 覆盖 `$HOME/.pc`）。

默认会以交互式 TUI 选择要安装的内置模板；如需脚本化（不交互）安装全部内置模板：

```bash
pc templates init --non-interactive
```

你也可以把常见技术栈“自由拼装”成一个新模板（仅生成你选择的部分，避免引入不需要的依赖）：

```bash
pc templates compose my-stack --interactive
```

或非交互：

```bash
pc templates compose my-stack --with python --with uv --with node --with pnpm --with go
```

### 1) 初始化当前目录的 devcontainer（如果还没有）

```bash
pc init . --preset python-uv
```

### 2) 从当前 git 仓库创建一个隔离的 worktree + 分支并启动 devcontainer

在任意 git 仓库目录内：

```bash
pc new feat/tui-templates
# 或等价写法：
pc agent new feat/tui-templates
```

要求当前仓库至少有 1 个 commit（否则 `git worktree` 会创建 orphan 分支，worktree 为空）。

如果 worktree 里没有 devcontainer 配置（例如没有 `.devcontainer/devcontainer.json`），`pc` 会用 `--preset`（默认 `python-uv`）走 “stealth” 模式：用 `devcontainer up --config` 指向 `$HOME/.pc/runtime/devcontainer-presets/<preset>/.devcontainer/devcontainer.json`（会优先读取 `$HOME/.pc/templates/<preset>/`，没有则用内置模板），不需要把 `.devcontainer/` 写进你的仓库/工作区。

说明：当分支名包含 `/` 等字符时，`pc` 会自动派生出一个合法的 `agent-name` 作为 worktree 目录名与 compose project 名；如需指定可用 `--agent-name <name>`。

默认 worktree 会创建在：`<repo>/../<repo-name>-agents/<agent-name>`，也可用 `--base-dir` 或环境变量 `AGENT_WORKTREE_BASE_DIR` 指定。

选择基分支（按最近更新排序，用上下键选择）：

```bash
pc agent new agent-a --select-base
```

或直接指定：

```bash
pc agent new agent-a --base main
```

可选启动桌面 sidecar：

```bash
pc agent new agent-a --desktop
```

### 3) 仅为某个 worktree 启动桌面 sidecar

```bash
pc agent desktop-on /path/to/worktree
```

或对任意目录：

```bash
pc desktop-on /path/to/dir
```

### 4) 删除 agent（停止 docker + 删除 worktree）

```bash
pc agent rm feat/tui-templates
```

说明：

- `pc agent rm` **只删除 worktree**，不会删除对应的 git 分支（如需删除可手动 `git branch -D <branch>`）。
- 如果 worktree 里存在未提交的修改或未追踪文件，`git worktree remove` 可能会提示需要 `--force`；`pc` 会展示 `git status --porcelain` 并让你选择是否重试（默认 `no`）。
- 模板里的共享缓存卷会标为 `external: true`，并在启动前自动 `docker volume create`（第一次运行会创建，后续复用），避免 Compose “created for project …” 的告警。

## 测试

普通集成测试（不需要真实 docker/devcontainer）：

```bash
CARGO_HOME=/tmp/cargo-home cargo test
```

实机 E2E 测试（需要 `git` + `devcontainer` + `docker` 且 docker daemon 可用，会真的拉镜像/启动容器）：

```bash
PC_E2E=1 CARGO_HOME=/tmp/cargo-home cargo test --test e2e_real -- --ignored --nocapture
```

覆盖率（需要 `llvm-tools-preview` + `cargo-llvm-cov`）：

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
cargo llvm-cov --locked --all-features --all-targets --workspace --summary-only
```
