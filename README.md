```
           ███   █████    █████       ███   █████   
          ░░░   ░░███    ░░███       ░░░   ░░███    
  ███████ ████  ███████   ░███ █████ ████  ███████  
 ███░░███░░███ ░░░███░    ░███░░███ ░░███ ░░░███░   
░███ ░███ ░███   ░███     ░██████░   ░███   ░███    
░███ ░███ ░███   ░███ ███ ░███░░███  ░███   ░███ ███
░░███████ █████  ░░█████  ████ █████ █████  ░░█████ 
 ░░░░░███░░░░░    ░░░░░  ░░░░ ░░░░░ ░░░░░    ░░░░░  
 ███ ░███                                           
░░██████                                            
 ░░░░░░                                             
```

[![CI](https://github.com/UniverLab/gitkit/actions/workflows/ci.yml/badge.svg)](https://github.com/UniverLab/gitkit/actions/workflows/ci.yml)
[![Release](https://github.com/UniverLab/gitkit/actions/workflows/release.yml/badge.svg)](https://github.com/UniverLab/gitkit/actions/workflows/release.yml)
[![Crates.io](https://img.shields.io/crates/v/gitkit)](https://crates.io/crates/gitkit)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Configure a git repo in seconds — hooks, `.gitignore`, `.gitattributes`, and git config. Interactive wizard or direct commands. No Node.js, no Python, no runtime dependencies. One binary.

---

## Installation

### Quick install (recommended)

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/UniverLab/gitkit/main/scripts/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/UniverLab/gitkit/main/scripts/install.ps1 | iex
```

### Via cargo

```bash
cargo install gitkit
```

Available on [crates.io](https://crates.io/crates/gitkit).

### GitHub Releases

Check the [Releases](https://github.com/UniverLab/gitkit/releases) page for precompiled binaries (Linux x86_64, macOS x86_64/ARM64, Windows x86_64).

### Uninstall

**Linux / macOS:**
```bash
rm -f ~/.local/bin/gitkit
```

**Windows (PowerShell):**
```powershell
Remove-Item "$env:LOCALAPPDATA\gitkit\gitkit.exe" -Force
```

---

## Quick Start 

**Clone and configure a repo in one command:**

```bash
gitkit clone https://github.com/user/repo
```

Or configure an existing repo:

```bash
gitkit init
```

Or use commands directly:

```bash
gitkit hooks add conventional-commits
gitkit ignore add rust,vscode,agentic
gitkit attributes init
gitkit config apply defaults
```

---

## `gitkit init`

Interactive wizard that guides you through configuring a repo step by step.

- Hooks — built-ins pre-selected, or add a custom command
- `.gitignore` — filterable search across all gitignore.io templates + built-ins
- `.gitattributes` — line endings and binary file presets
- Git config — 6 individual options, recommended ones pre-selected

Automatically initializes a git repository if one doesn't exist:

```bash
gitkit init
```

---

## `gitkit clone`

Clone a repository and automatically run `gitkit init` to configure it.

**Usage:**

```bash
gitkit clone [OPTIONS] <REPOSITORY> [DIRECTORY]
```

**Arguments:**

- `<REPOSITORY>` — Repository URL or path to clone
- `[DIRECTORY]` — Target directory (defaults to repository name)

**Options:**

- `-b, --branch <BRANCH>` — Clone specific branch (defaults to repository default)
- `-h, --help` — Print help

**Examples:**

```bash
# Clone and auto-configure
gitkit clone https://github.com/user/repo

# Clone specific branch
gitkit clone -b develop https://github.com/user/repo

# Clone to custom directory
gitkit clone https://github.com/user/repo my-project
```

The wizard runs automatically after cloning, allowing you to configure hooks, `.gitignore`, `.gitattributes`, and git config in one workflow.

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
| `gitkit config apply delta` | `core.pager delta` (requires `cargo`) |

---

## Built-in Hooks

Run `gitkit hooks list --available` to see these without leaving the terminal.

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

## License

MIT
