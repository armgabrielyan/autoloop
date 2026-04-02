# autoloop

[![crates.io](https://img.shields.io/crates/v/autoloop.svg)](https://crates.io/crates/autoloop)
[![crates.io downloads](https://img.shields.io/crates/d/autoloop.svg)](https://crates.io/crates/autoloop)
[![npm version](https://img.shields.io/npm/v/%40armgabrielyan%2Fautoloop)](https://www.npmjs.com/package/@armgabrielyan/autoloop)
[![npm downloads](https://img.shields.io/npm/dm/%40armgabrielyan%2Fautoloop)](https://www.npmjs.com/package/@armgabrielyan/autoloop)
[![CI](https://github.com/armgabrielyan/autoloop/actions/workflows/ci.yml/badge.svg)](https://github.com/armgabrielyan/autoloop/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

**Agent-agnostic iterative optimization loops for coding agents.**

`autoloop` is the missing runtime for “let the agent iterate on this overnight.”

Inspired by [Karpathy's `autoresearch`](https://github.com/karpathy/autoresearch), AutoLoop takes the same core idea and generalizes it beyond a single training script:

- instead of one model-training repo, it works on arbitrary codebases
- instead of one hardcoded metric loop, it uses your repo's eval command and guardrails
- instead of one agent setup, it installs wrappers for multiple coding agents
- instead of “try random changes and hope,” it keeps measured wins and discards regressions

Use it when you want agents to:

- run bounded optimization loops on real repos
- improve a benchmark or score without breaking correctness
- leave behind commits, learnings, and reviewable history
- work through a repeatable state machine instead of an improvised workflow

## Table of Contents

- [Demo](#-demo)
- [Features](#-features)
- [Installation](#-installation)
  - [Quick Install](#-quick-install-macoslinux)
  - [Homebrew](#-homebrew-macoslinux)
  - [npm](#-npmnpx)
  - [Cargo](#-cargo)
  - [Manual Download](#-manual-download)
- [Build from Source](#-build-from-source)
- [Quick Start](#-quick-start)
- [Running the Agent](#-running-the-agent)
- [The Idea](#-the-idea)
- [How It Works](#-how-it-works)
- [Agent Integrations](#-agent-integrations)
- [Direct CLI Workflow](#-direct-cli-workflow)
- [Usage Examples](#-usage-examples)
- [Example Repos and Docs](#-example-repos-and-docs)
- [Project Structure](#-project-structure)
- [Design Choices](#-design-choices)
- [Supported Detection](#-supported-detection)
- [Star History](#-star-history)
- [License](#-license)

## 🎬 Demo

Screenshots from a real bounded run on [`examples/smoke-python-search`](examples/smoke-python-search/README.md):

### 1. The agent makes a small change, evaluates it, and records a kept win

![AutoLoop keep flow](https://raw.githubusercontent.com/armgabrielyan/autoloop/main/assets/1.png)

### 2. The run ends with a measurable result and clean experiment history

![AutoLoop status](https://raw.githubusercontent.com/armgabrielyan/autoloop/main/assets/2.png)

### 3. AutoLoop refreshes concrete learnings from the session

![AutoLoop learnings](https://raw.githubusercontent.com/armgabrielyan/autoloop/main/assets/3.png)

The high-level flow looks like this:

```bash
# Bootstrap and verify the repo-specific config
autoloop init --verify
autoloop doctor --fix
autoloop baseline

# Install agent wrappers for your preferred runtime
autoloop install codex
# autoloop install claude-code
# autoloop install cursor
# autoloop install opencode
# autoloop install gemini-cli
# autoloop install generic

# Then tell the agent:
# "Use `autoloop-run` to reduce benchmark latency in this repo.
#  Keep behavior unchanged. Use at most 5 experiments."

# Review what happened afterward
autoloop status --all
autoloop learn --all
autoloop finalize --session
```

For a real end-to-end fixture walkthrough, see [docs/examples/real-workflow.md](docs/examples/real-workflow.md).

## ✨ Features

- 🤖 **Autonomous bounded runs** — Install `autoloop-run` wrappers so agents can initialize, verify, baseline, iterate, learn, and stop without per-experiment user input
- 🔎 **Repo-aware setup inference** — Detects likely eval and guardrail commands for Rust, Python, Node/Bun, Go, JVM, and .NET repos
- 🩺 **Config verification and repair** — `autoloop doctor` runs the inferred commands for real and can rewrite a broken config with a verified inferred candidate
- 📏 **Metric-driven evals** — Supports `METRIC name=value`, JSON extraction, and regex-based parsing
- 🛡️ **Guardrails and verdicts** — Evals combine the primary metric, pass/fail or metric guardrails, and confidence thresholds into `keep`, `discard`, or `rerun`
- 🧠 **Experiment memory** — Records experiments, outcomes, tags, learnings, and session summaries under `.autoloop/`
- 🌿 **Path-scoped git actions** — `keep --commit` and `discard --revert` operate on recorded experiment paths instead of sweeping the whole worktree
- 🔀 **Review finalization** — `autoloop finalize` groups committed wins into clean review branches
- 🧩 **Agent integrations** — Generates workspace-local context/skills/commands for Codex, Claude Code, Cursor, OpenCode, Gemini CLI, and generic agent setups
- 🧾 **Human + JSON output** — Every command can be consumed by both people and agents

## 📦 Installation

### ⚡ Quick Install (macOS/Linux)

```bash
curl -sSf https://raw.githubusercontent.com/armgabrielyan/autoloop/main/install.sh | sh
```

### 🍺 Homebrew (macOS/Linux)

```bash
brew install armgabrielyan/tap/autoloop
```

### 📦 npm/npx

```bash
# Install globally
npm install -g @armgabrielyan/autoloop

# Or run directly
npx @armgabrielyan/autoloop --help
```

### 🦀 Cargo

```bash
cargo install autoloop
```

### ⬇️ Manual Download

Download pre-built binaries from the [GitHub Releases](https://github.com/armgabrielyan/autoloop/releases) page.

| Platform | Architecture | Download |
|----------|--------------|----------|
| Linux | x86_64 (glibc) | `autoloop-VERSION-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | x86_64 (musl/static) | `autoloop-VERSION-x86_64-unknown-linux-musl.tar.gz` |
| Linux | ARM64 | `autoloop-VERSION-aarch64-unknown-linux-gnu.tar.gz` |
| macOS | Intel | `autoloop-VERSION-x86_64-apple-darwin.tar.gz` |
| macOS | Apple Silicon | `autoloop-VERSION-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `autoloop-VERSION-x86_64-pc-windows-msvc.zip` |

### 🔨 Build from Source

```bash
git clone https://github.com/armgabrielyan/autoloop
cd autoloop
cargo build --release
# Binary will be at target/release/autoloop
```

## 🚀 Quick Start

The normal end-user flow is:

```bash
# 1. Initialize and verify the repo-specific config
autoloop init --verify

# 2. If init left the config unhealthy, repair it
autoloop doctor --fix

# 3. Record a baseline
autoloop baseline

# 4. Install an agent integration
autoloop install codex
#    or: autoloop install claude-code
#    or: autoloop install cursor
#    or: autoloop install opencode
#    or: autoloop install gemini-cli
#    or: autoloop install generic
```

Then invoke the installed wrapper in your agent. For example, in Codex:

```text
Use `autoloop-run` to reduce the benchmark latency in this repo. Keep behavior unchanged. Use at most 5 experiments and ask me only if you are genuinely blocked.
```

## 🧠 Running the Agent

Once setup is complete, the intended user experience is not “drive every subcommand manually.” The intended experience is:

1. the user starts a bounded run once
2. the agent owns the experiment loop
3. the user checks status, learnings, and final review branches afterward

Example prompts:

### Codex

```text
Use `autoloop-run` to reduce the benchmark latency in this repo. Keep behavior unchanged. Use at most 5 experiments and ask me only if you are genuinely blocked.
```

### Claude Code

```text
Run `/autoloop-run` for this repo. Reduce the benchmark latency, keep behavior unchanged, use at most 5 experiments, and only ask me if you are genuinely blocked.
```

### Cursor

```text
Use `autoloop-run` in this workspace to run a bounded optimization loop. Reduce the benchmark latency, keep behavior unchanged, and stop after at most 5 experiments unless you are blocked earlier.
```

### OpenCode

```text
Use `autoloop-run` to optimize this repo in a bounded loop. Preserve behavior, use at most 5 experiments, and only ask for input if you are genuinely blocked.
```

### Gemini CLI

```text
Use `autoloop-run` to reduce the benchmark latency in this repo. Keep behavior unchanged, use at most 5 experiments, and minimize user interaction.
```

### Generic

```text
Use the AutoLoop program in this repo to run a bounded optimization loop. Reduce benchmark latency, preserve behavior, limit the run to 5 experiments, and keep user interaction minimal.
```

## 💡 The Idea

The core idea is simple:

give an AI coding agent a real repo, a real eval command, a real correctness guardrail, and let it experiment autonomously in a bounded loop.

Each iteration is small and attributable:

1. propose one experiment
2. make one change
3. run the metric and guardrails
4. keep the win or discard the regression
5. record the result
6. repeat

You wake up to:

- a baseline
- a history of experiments
- concrete learnings
- committed wins or discarded dead ends
- reviewable branches instead of a vague “the agent tried stuff”

That is the `autoresearch` idea translated from a specific training script into general software-engineering repos.

## ⚙️ How It Works

AutoLoop separates the optimization loop into a few explicit phases:

- **Setup**: `autoloop init --verify` infers `.autoloop/config.toml` from the repo, and `autoloop doctor` proves whether the inferred commands actually work
- **Baseline**: `autoloop baseline` records the starting metric and any metric-style guardrail baselines
- **Loop**: the agent runs bounded experiments using `pre`, `eval`, `keep`, and `discard`
- **Learning**: `autoloop learn` refreshes `.autoloop/learnings.md` from recorded outcomes
- **Finalization**: `autoloop finalize` turns committed wins into clean review branches

The installed wrappers like `autoloop-run` sit above this low-level CLI and orchestrate the loop automatically.

## 🤖 Agent Integrations

AutoLoop generates workspace-local wrappers instead of assuming one specific agent runtime.

```bash
autoloop install codex
autoloop install claude-code
autoloop install cursor
autoloop install opencode
autoloop install gemini-cli
autoloop install generic
```

Installed wrapper responsibilities:

- `autoloop-init`
- `autoloop-doctor`
- `autoloop-baseline`
- `autoloop-run`
- `autoloop-status`
- `autoloop-learn`
- `autoloop-finalize`

The high-level wrapper is `autoloop-run`. The lower-level CLI remains available when you want direct control.

## 🛠️ Direct CLI Workflow

If you want to drive the loop manually instead of through an installed wrapper:

```bash
autoloop init --verify
autoloop doctor --fix
autoloop baseline
autoloop session start --name "latency-pass"
autoloop pre --description "Cache normalized tokens once per search call"
# make one small code change
autoloop eval --json
autoloop keep --commit --description "Cache normalized tokens once per search call"
autoloop learn --session
autoloop session end
autoloop finalize --session
```

## 📚 Usage Examples

### Verify and repair config

```bash
autoloop init --verify
autoloop doctor --json
autoloop doctor --fix
```

### Track loop status

```bash
autoloop status
autoloop status --all
autoloop learn --session
autoloop learn --all
```

### Resolve a pending eval

```bash
autoloop eval --json
autoloop keep --commit --description "Reduce repeated normalization work"

# or

autoloop discard --revert \
  --description "Try a one-pass scoring rewrite" \
  --reason "Slower than the kept version"
```

### Create review branches from wins

```bash
autoloop finalize --session
autoloop finalize --all
```

## 🧪 Example Repos and Docs

Use the example fixtures and workflow docs as the real regression bar:

- [Real workflow example](docs/examples/real-workflow.md)
- [Acceptance matrix](docs/examples/acceptance-matrix.md)
- [Smoke Python search fixture](examples/smoke-python-search/README.md)
- [Smoke Rust CLI fixture](examples/smoke-rust-cli/README.md)

The intended progression is:

1. `examples/smoke-python-search`
2. `examples/smoke-rust-cli`

Those two fixtures exercise the full promised path:

- no pre-existing `.autoloop/`
- repo-aware config inference
- verified baselining
- bounded autonomous loop
- recorded keep/discard history
- refreshed `.autoloop/learnings.md`

## 🗂️ Project Structure

At a high level, these are the parts that matter most:

```text
src/
  commands/        # user-facing CLI commands
  detect.rs        # repo-aware config inference
  validation.rs    # doctor/init verification logic
  integrations.rs  # generated agent wrapper files
  git.rs           # path-scoped keep/discard/finalize git helpers
  state.rs         # .autoloop persisted state
  experiments.rs   # experiment history and learning analysis

integrations/_shared/
  # shared wrapper contracts for installed agent integrations

examples/
  smoke-python-search/
  smoke-rust-cli/

docs/examples/
  real-workflow.md
  acceptance-matrix.md
```

The main runtime state for a user repo lives under `.autoloop/`, not in the AutoLoop source repo itself.

## 🧭 Design Choices

- **Bounded loops, not infinite chaos**: AutoLoop is optimized for runs like “do at most 5 experiments,” not uncontrolled forever-agents
- **Eval command is the source of truth**: improvements are measured against what the repo actually runs, not guessed heuristics
- **Guardrails are first-class**: a faster benchmark is not enough if the correctness command fails
- **Recorded keep/discard outcomes**: every iteration should end in a resolved state, not a pile of half-measured changes
- **Path-scoped git actions**: keep/discard operate on recorded experiment paths so unrelated repo noise does not derail the loop
- **Agent-agnostic wrappers**: AutoLoop does not assume one blessed agent runtime; it installs local wrappers for several
- **Human and machine output**: every command is readable in the terminal and consumable via `--json`

## 🌐 Supported Detection

Current setup inference includes:

- **Rust**: `Cargo.toml`, benchmark binaries, `cargo test`
- **Python**: `pyproject.toml`, `uv`, `poetry`, `pipenv`, `hatch`, `pytest`, `unittest`, `tox`, `nox`
- **Node/TypeScript**: `npm`, `pnpm`, `yarn`, `bun`, script-based eval/test commands
- **Go**: `go.mod`, `go test`, benchmark packages, `go run ./cmd/...`
- **JVM**: Gradle and Maven repos, test tasks, JMH-style eval inference
- **.NET**: `.sln`, `.csproj`, `dotnet run`, `dotnet test`

Inference is intentionally a first pass. `autoloop doctor` is the command that proves whether the inferred setup is actually healthy in the current repo.

## ⭐ Star History

[![Star History Chart](https://api.star-history.com/svg?repos=armgabrielyan/autoloop&type=Date)](https://star-history.com/#armgabrielyan/autoloop&Date)

## 📄 License

MIT
