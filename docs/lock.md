---
title: Lock
description: Block commits locally and reversibly for the duration of an agent session.
order: 5
---

# Lock

`gitkit lock` lets a human stop an AI agent (or anyone else) from
committing to a repository, locally, for the duration of a session.

```bash
gitkit lock                              # block commits until `gitkit unlock`
gitkit lock --reason "Agent session"     # custom message shown on a blocked commit
gitkit lock --timeout 30m                # auto-expires after 30 minutes
gitkit lock status                       # show whether a lock is active
gitkit unlock                            # remove the lock
```

Locking twice updates the existing lock (reason, timeout) instead of
stacking or erroring.

## How it works

`gitkit lock` writes a small JSON state file at `.git/gitkit.lock` and
installs a `pre-commit` hook that reads it. The hook is pure POSIX `sh` —
no dependency on the `gitkit` binary — so it stays fast on every commit.
A missing, empty, or malformed lock file is always treated as unlocked:
a corrupt lock never blocks a commit.

If you already had a `pre-commit` hook, it is backed up to
`pre-commit.gitkit-orig` and chained to — it still runs after the lock
check passes. `gitkit unlock` restores it and removes the backup.

The lock is per-repository, local only, and never committed or pushed —
it lives entirely under `.git/`.

## Limitation: `--no-verify`

`git commit --no-verify` bypasses all pre-commit hooks, including this
one. **This is expected and not treated as a bug.** The lock's threat
model is an AI agent following its instructions, not a human deliberately
working around a local safeguard — so no attempt is made to defend
against `--no-verify`. If you need a guarantee that survives a
determined bypass, this is not that guarantee.
