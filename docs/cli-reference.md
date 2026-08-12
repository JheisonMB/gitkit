---
title: CLI Reference
description: Every gitkit command and flag.
order: 8
---

# CLI Reference

```
gitkit [command] [options]
```

Running `gitkit` with no command starts the interactive wizard.

## Core

| Command | Description |
|---|---|
| `gitkit` / `gitkit init` | Interactive wizard to configure the repo |
| `gitkit status` | Show current configuration status |
| `gitkit clone <repo> [dir]` | Clone a repository and run the wizard |
| `gitkit clone -b <branch> <repo>` | Clone a specific branch |

## Hooks

| Command | Description |
|---|---|
| `gitkit hooks add <builtin>` | Install a built-in hook (hook type inferred) |
| `gitkit hooks add <hook> <command>` | Install a custom shell command as a hook |
| `gitkit hooks list` | List installed hooks |
| `gitkit hooks list --available` | Show all built-in hooks with descriptions |
| `gitkit hooks remove <hook>` | Remove an installed hook |
| `gitkit hooks show <hook>` | Print hook content |

## Lock

| Command | Description |
|---|---|
| `gitkit lock` | Block commits until `gitkit unlock` |
| `gitkit lock --reason <msg>` | Set the message shown on a blocked commit |
| `gitkit lock --timeout <duration>` | Auto-expire the lock, e.g. `30m`, `2h` |
| `gitkit lock --push` | Also block pushes (in addition to commits) |
| `gitkit lock --all` | Block both commits and pushes |
| `gitkit lock status` | Show whether a lock is active, its reason and expiry |
| `gitkit lock status --json` | Show lock status as machine-readable JSON with exit code signal |
| `gitkit unlock` | Remove the lock and restore any backed-up hook |

`git commit --no-verify` and `git push --no-verify` bypass the lock — see [Lock](lock.md) for why
that is accepted rather than defended against.

## Ignore

| Command | Description |
|---|---|
| `gitkit ignore add <templates>` | Generate/merge `.gitignore` via gitignore.io |
| `gitkit ignore list [filter]` | List available templates |

## Attributes

| Command | Description |
|---|---|
| `gitkit attributes init` | Apply line-endings preset to `.gitattributes` |

## Config

| Command | Description |
|---|---|
| `gitkit config apply defaults` | `push.autoSetupRemote`, `help.autocorrect`, `diff.algorithm` |
| `gitkit config apply advanced` | `merge.conflictstyle zdiff3`, `rerere.enabled` |
| `gitkit config apply delta` | `core.pager delta` |
| `gitkit config show` | Show current git config values |

Scope: `--global` (all repos) or `--local` (current repo). Default is
`--local` inside a repo, `--global` otherwise.

## Builds

| Command | Description |
|---|---|
| `gitkit build list` | List saved builds |
| `gitkit build save <name>` | Save current repo config as a build |
| `gitkit build apply <name>` | Apply a saved build |
| `gitkit build delete <name>` | Delete a saved build |

## Global flags

| Flag | Description |
|---|---|
| `--yes`, `-y` | Skip confirmation prompts |
| `--force`, `-f` | Overwrite existing files |
| `--dry-run` | Preview changes without applying |
| `--help` | Show help for any command |
| `--version` | Show gitkit version |
