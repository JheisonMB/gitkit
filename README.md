# gitkit

[![CI](https://github.com/JheisonMB/gitkit/actions/workflows/ci.yml/badge.svg)](https://github.com/JheisonMB/gitkit/actions/workflows/ci.yml)
[![Release](https://github.com/JheisonMB/gitkit/actions/workflows/release.yml/badge.svg)](https://github.com/JheisonMB/gitkit/actions/workflows/release.yml)
[![Crates.io](https://img.shields.io/crates/v/gitkit)](https://crates.io/crates/gitkit)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Standalone CLI for configuring git repos — hooks, .gitignore, and .gitattributes. No Node.js, no Python, no runtime dependencies. One binary.

---

## Installation

### Quick install (recommended)

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/JheisonMB/gitkit/main/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/JheisonMB/gitkit/main/install.ps1 | iex
```

### Via cargo

```bash
cargo install gitkit
```

### GitHub Releases

Check the [Releases](https://github.com/JheisonMB/gitkit/releases) page for precompiled binaries (Linux x86_64, macOS x86_64/ARM64, Windows x86_64).

### Uninstall

```bash
rm -f ~/.local/bin/gitkit
```

---

## Quick Start

```bash
# Install a built-in hook
gitkit hooks init commit-msg conventional-commits

# Install a custom hook command
gitkit hooks init pre-push "cargo test"

# List installed hooks
gitkit hooks list

# Generate a .gitignore
gitkit ignore add rust,vscode

# Apply line endings preset
gitkit attributes init
```

---

## Commands

| Command | Description |
|---|---|
| `gitkit hooks init <hook> <builtin\|command>` | Install a hook (built-in or custom command) |
| `gitkit hooks list` | List installed hooks |
| `gitkit hooks remove <hook>` | Remove a hook |
| `gitkit hooks show <hook>` | Show hook content |
| `gitkit ignore add <templates>` | Generate .gitignore via gitignore.io |
| `gitkit ignore list [filter]` | List available templates |
| `gitkit attributes init` | Apply line endings preset |
| `gitkit config apply <preset>` | Apply git config preset (defaults, advanced, delta) |

---

## Built-in Hooks

| Name | Hook | Description |
|---|---|---|
| `conventional-commits` | `commit-msg` | Validates Conventional Commits format |
| `no-secrets` | `pre-commit` | Detects common secret patterns |
| `branch-naming` | `pre-commit` | Validates branch name pattern |

---

## Global Flags

| Flag | Description |
|---|---|
| `--yes`, `-y` | Skip confirmation prompts |
| `--force`, `-f` | Overwrite existing files |
| `--dry-run` | Preview changes without applying |

---

## Tech Stack

| Concern | Crate |
|---|---|
| CLI parsing | `clap` (derive) |
| Error handling | `anyhow` |
| HTTP client | `ureq` |

---

## License

MIT
