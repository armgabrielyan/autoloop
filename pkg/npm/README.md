# @armgabrielyan/autoloop npm package

This is the npm distribution package for [autoloop](https://github.com/armgabrielyan/autoloop), a CLI for agent-agnostic optimization loops.

## Installation

```bash
npm install -g @armgabrielyan/autoloop
```

For one-off usage with `npx`:

```bash
npx @armgabrielyan/autoloop --help
```

## What this package does

When you install this package, it automatically downloads the appropriate pre-built binary for your platform from GitHub Releases. Supported platforms:

- macOS (Intel and Apple Silicon)
- Linux (x64 and ARM64)
- Windows (x64)

## Alternative installation methods

### Shell installer

```bash
curl -sSf https://raw.githubusercontent.com/armgabrielyan/autoloop/main/install.sh | sh
```

### Cargo

```bash
cargo install autoloop
```

### Homebrew

```bash
brew install armgabrielyan/tap/autoloop
```

## Usage

```bash
autoloop init --verify
autoloop baseline
autoloop install codex
```

For more information, see the [full documentation](https://github.com/armgabrielyan/autoloop).

## License

MIT
