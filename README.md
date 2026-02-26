# pc-cli (`pc`)

[![CI](https://github.com/BetterAndBetterII/parallel-coding/actions/workflows/ci.yml/badge.svg)](https://github.com/BetterAndBetterII/parallel-coding/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/BetterAndBetterII/parallel-coding/branch/main/graph/badge.svg)](https://codecov.io/gh/BetterAndBetterII/parallel-coding)

一个可安装的 Rust CLI，用于在本机通过 **git worktree** 管理并行任务目录，并可选自动用 **VS Code** 打开新 worktree。

## 安装

### Linux (Debian/Ubuntu) 一键安装（从 GitHub Release 下载二进制）

当前提供：`x86_64-unknown-linux-gnu`。

```bash
PC_VERSION="$(git ls-remote --tags --refs --sort='-v:refname' https://github.com/BetterAndBetterII/parallel-coding.git 'v*' | head -n1 | awk -F/ '{print $3}')" && curl -fsSL "https://github.com/BetterAndBetterII/parallel-coding/releases/download/${PC_VERSION}/pc-x86_64-unknown-linux-gnu" -o /tmp/pc && sudo install -m 755 /tmp/pc /usr/local/bin/pc && rm /tmp/pc
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

> 提示：本仓库提供 `rust-toolchain.toml`，用 `rustup` 时会自动选择并安装对应工具链。

## 依赖

- `git`
- `code`（可选：`pc new` 自动打开 VS Code 新窗口）

## 用法（常用）

### 1) 从当前 git 仓库创建一个隔离的 worktree + 分支

在任意 git 仓库目录内：

```bash
pc new feat/codex
```

要求当前仓库至少有 1 个 commit（否则 `git worktree` 会创建 orphan 分支，worktree 为空）。

说明：当分支名包含 `/` 等字符时，`pc` 会自动派生出一个合法的 `agent-name` 作为 worktree 目录名；如需指定可用 `--agent-name <name>`。

默认 worktree 会创建在：`<repo>/../<repo-name>-agents/<agent-name>`，也可用 `--base-dir` 或环境变量 `AGENT_WORKTREE_BASE_DIR` 指定。

选择基分支（按最近更新排序，用上下键选择）：

```bash
pc new feat/codex --select-base
```

或直接指定：

```bash
pc new feat/codex --base main
```

### 2) 删除 worktree（保留分支）

```bash
pc rm feat/codex
```

或不传分支名（仅 TTY），从现有 worktree 列表中选择：

```bash
pc rm
```

说明：

- `pc rm` **只删除 worktree**，不会删除对应的 git 分支（如需删除可手动 `git branch -D <branch>`）。
- 为避免误删，在 TTY 下会进行二次确认（确认 + 需要手动输入一次目标名称）。
- 如果 worktree 里存在未提交的修改或未追踪文件，`git worktree remove` 可能会提示需要 `--force`；`pc` 会展示 `git status --porcelain` 并让你选择是否重试（默认 `no`）。

## 测试

普通集成测试：

```bash
cargo test --locked
```

也可以用本仓库的 cargo alias：

- `cargo fmt-check`
- `cargo lint`
- `cargo test-locked`

覆盖率（需要 `llvm-tools-preview` + `cargo-llvm-cov`）：

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
cargo llvm-cov --locked --all-features --all-targets --workspace --summary-only
```
