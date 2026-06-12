---
title: Quick Start
description: The wizard, the status overview, and clone-and-configure in one command.
order: 3
---

# Quick Start

## The wizard

Run gitkit with no arguments inside (or outside) a repository:

```bash
gitkit
# or explicitly:
gitkit init
```

The wizard guides you step by step and shows what is already configured:

- **Hooks** — shows installed hooks, pre-selects them, allows removal.
- **`.gitignore`** — filterable search across all gitignore.io templates
  plus built-ins.
- **`.gitattributes`** — line endings and binary file presets.
- **Git config** — shows current values, allows removal.
- **Custom hooks** — interactive picker for hook type selection.

If the current directory is not a git repository, gitkit initializes one
automatically.

## Clone and configure in one command

```bash
gitkit clone https://github.com/user/repo            # clone + wizard
gitkit clone -b develop https://github.com/user/repo # specific branch
gitkit clone https://github.com/user/repo my-project # custom directory
```

The wizard runs automatically after cloning.

## Direct commands

Everything the wizard does is also available as direct commands:

```bash
gitkit hooks add conventional-commits
gitkit ignore add rust,vscode,agentic
gitkit attributes init
gitkit config apply defaults
```

## Check the result

```bash
gitkit status
```

```
Hooks:
  ✓ conventional-commits (commit-msg)
  ✓ custom: pre-push → "cargo test"

.gitignore:
  ✓ 14 patterns

.gitattributes:
  ✓ line-endings (eol=lf)

Git config (global):
  ✓ push.autoSetupRemote = true
  ✓ help.autocorrect = prompt
  ✓ diff.algorithm = histogram
```

When you are happy with a setup, [save it as a build](builds.md) and apply
it to every future project with one command.
