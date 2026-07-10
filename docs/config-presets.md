---
title: Config Presets
description: Curated git config presets with scope control and idempotency detection.
order: 6
---

# Config Presets

Gitkit ships curated git config presets — small sets of options that make
day-to-day git noticeably better:

| Preset | What it sets |
|---|---|
| `defaults` | `push.autoSetupRemote`, `help.autocorrect`, `diff.algorithm histogram` |
| `advanced` | `merge.conflictstyle zdiff3`, `rerere.enabled` |
| `delta` | `core.pager delta` (requires [delta](https://github.com/dandavison/delta)) |

```bash
gitkit config apply defaults
gitkit config apply advanced
gitkit config show          # current git config values
```

## Scope

- `--global` — apply to global git config (all repos)
- `--local` — apply to the current repo only
- Default: `--local` if inside a repo, `--global` otherwise

## Idempotency

Options already set with the same value are detected and skipped:

```
$ gitkit config apply defaults --global
✓ push.autoSetupRemote = true (already set)
✓ help.autocorrect = prompt (already set)
✓ diff.algorithm = histogram (already set)

All configs already applied.
```

Running a preset twice never duplicates or clobbers anything.
