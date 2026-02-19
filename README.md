# pc-cli (`pc`)

一个可安装的 Rust CLI，用于在本机用 **git worktree + Dev Container** 管理“并行 Agent/任务环境”，并可选启动每个任务独立的 **webtop 桌面容器**用于浏览器调试。

## 安装

需要本机先安装 Rust（`cargo`）。

在仓库目录内：

```bash
cargo install --path .
```

安装后可在任意目录使用：

```bash
pc --help
```

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

### 1) 初始化当前目录的 devcontainer（如果还没有）

```bash
pc init . --preset python-uv
```

### 2) 从当前 git 仓库创建一个隔离的 worktree + 分支并启动 devcontainer

在任意 git 仓库目录内：

```bash
pc agent new agent-a
```

要求当前仓库至少有 1 个 commit（否则 `git worktree` 会创建 orphan 分支，worktree 为空，进而找不到 `.devcontainer/devcontainer.json`）。

默认 worktree 会创建在：`<repo>/../<repo-name>-agents/<agent-name>`，也可用 `--base-dir` 或环境变量 `AGENT_WORKTREE_BASE_DIR` 指定。

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
