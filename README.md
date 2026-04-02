# gitkit

[![CI](https://github.com/JheisonMB/gitkit/actions/workflows/ci.yml/badge.svg)](https://github.com/JheisonMB/gitkit/actions/workflows/ci.yml)
[![Release](https://github.com/JheisonMB/gitkit/actions/workflows/release.yml/badge.svg)](https://github.com/JheisonMB/gitkit/actions/workflows/release.yml)
[![Crates.io](https://img.shields.io/crates/v/gitkit)](https://crates.io/crates/gitkit)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Standalone CLI for configuring git repos — hooks, `.gitignore`, and `.gitattributes`. No Node.js, no Python, no runtime dependencies. One binary.

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
# Install a built-in hook (hook name inferred automatically)
gitkit hooks add conventional-commits

# Install a custom hook command
gitkit hooks add pre-push "cargo test"

# See all available built-in hooks
gitkit hooks list --available

# List installed hooks
gitkit hooks list

# Generate a .gitignore (merges with existing, no duplicates)
gitkit ignore add rust,vscode,agentic

# Apply line endings preset
gitkit attributes init

# Apply curated git config
gitkit config apply defaults
```

---

## Commands

### Hooks

| Command | Description |
|---|---|
| `gitkit hooks add <builtin>` | Install a built-in hook (hook name inferred) |
| `gitkit hooks add <hook> <command>` | Install a custom shell command as a hook |
| `gitkit hooks list` | List installed hooks |
| `gitkit hooks list --available` | Show all built-in hooks with descriptions |
| `gitkit hooks remove <hook>` | Remove an installed hook |
| `gitkit hooks show <hook>` | Print hook content |

### Ignore

| Command | Description |
|---|---|
| `gitkit ignore add <templates>` | Generate/merge `.gitignore` via gitignore.io |
| `gitkit ignore list [filter]` | List available templates |

### Attributes

| Command | Description |
|---|---|
| `gitkit attributes init` | Apply line endings preset to `.gitattributes` |

### Config

| Command | Description |
|---|---|
| `gitkit config apply defaults` | `push.autoSetupRemote`, `help.autocorrect`, `diff.algorithm` |
| `gitkit config apply advanced` | `merge.conflictstyle zdiff3`, `rerere.enabled` |
| `gitkit config apply delta` | `core.pager delta` (installs `git-delta` if needed) |

---

## Built-in Hooks

Run `gitkit hooks list --available` to see these at any time without leaving the terminal.

| Name | Hook | Description |
|---|---|---|
| `conventional-commits` | `commit-msg` | Validates Conventional Commits format |
| `no-secrets` | `pre-commit` | Detects common secret patterns in staged changes |
| `branch-naming` | `pre-commit` | Validates branch name matches convention |

Built-ins are embedded in the binary — no network required.

---

## Global Flags

| Flag | Description |
|---|---|
| `--yes`, `-y` | Skip confirmation prompts |
| `--force`, `-f` | Overwrite existing files |
| `--dry-run` | Preview changes without applying |

---

## Examples

```bash
# Set up a new repo in one go
gitkit hooks add conventional-commits
gitkit hooks add no-secrets
gitkit ignore add rust,vscode,agentic
gitkit attributes init
gitkit config apply defaults

# Preview what config apply would do
gitkit config apply delta --dry-run

# See what's installed
gitkit hooks list

# Discover built-ins without opening the docs
gitkit hooks list --available
```

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
