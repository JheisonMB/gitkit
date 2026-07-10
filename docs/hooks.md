---
title: Hooks
description: Built-in hooks (conventional commits, secret detection, branch naming) and custom shell commands.
order: 4
---

# Hooks

## Built-in hooks

Built-ins are embedded in the binary — no network required.

| Name | Hook | Description |
|---|---|---|
| `conventional-commits` | `commit-msg` | Validates Conventional Commits format |
| `no-secrets` | `pre-commit` | Detects common secret patterns in staged changes |
| `branch-naming` | `pre-commit` | Validates branch name matches convention |

```bash
gitkit hooks list --available   # see all built-ins with descriptions
gitkit hooks add no-secrets     # install one (hook type inferred)
```

## Custom hooks

Wire any shell command into a git hook:

```bash
gitkit hooks add pre-push "cargo test"
```

## Managing hooks

```bash
gitkit hooks list            # list installed hooks
gitkit hooks show <hook>     # print hook content
gitkit hooks remove <hook>   # remove an installed hook
```

The `gitkit` wizard also shows installed hooks, pre-selects them and
allows removal interactively.
