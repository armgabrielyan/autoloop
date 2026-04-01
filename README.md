# autoloop

AutoLoop is an agent-agnostic CLI for bounded optimization loops.

It helps agents initialize a workspace, verify and repair config, record a baseline, run iterative experiments, learn from history, and finalize reviewable wins.

## Installation

### Shell installer

```bash
curl -sSf https://raw.githubusercontent.com/armgabrielyan/autoloop/main/install.sh | sh
```

### npm

```bash
npm install -g @armgabrielyan/autoloop
```

### Cargo

```bash
cargo install autoloop
```

### Homebrew

```bash
brew install armgabrielyan/tap/autoloop
```

## Quick start

```bash
autoloop init --verify
autoloop baseline
autoloop install codex
```

Then invoke the installed wrapper in your agent, for example `autoloop-run`.

## Examples

- [Python and Rust smoke workflows](docs/examples/real-workflow.md)
- [Acceptance matrix](docs/examples/acceptance-matrix.md)

## License

MIT
