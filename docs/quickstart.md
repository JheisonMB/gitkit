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
  ✓ conventional-commits (commit-msg) — active
  ~ pre-push — modified: "cargo test"

.gitignore:
  ✓ 14 patterns

.gitattributes:
  ✓ line-endings (eol=lf)

Git config (global):
  ✓ push.autoSetupRemote = true
  ✓ help.autocorrect = prompt
  ✓ diff.algorithm = histogram
```

Each hook is reported as **active** (installed, executable, git runs it),
**dormant** (installed but not executable — git silently ignores it),
**modified** (content doesn't match a built-in, e.g. a custom command or a
hand-edited script), or simply absent from the list if nothing is installed
for that hook. Fix a dormant hook with:

```bash
gitkit status --repair   # sets the executable bit on every dormant hook
gitkit status --strict   # exits non-zero if any hook is dormant (for CI)
```

`--repair` only touches dormant hooks — it never rewrites content, and it
never installs a hook that was removed on purpose. See
[hooks.md](hooks.md#hook-health) for details.

When you are happy with a setup, [save it as a build](builds.md) and apply
it to every future project with one command.
