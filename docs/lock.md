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
gitkit lock --push                       # block pushes instead of commits
gitkit lock --all                        # block both commits and pushes
gitkit lock --reason "Agent session"     # custom message shown on a blocked operation
gitkit lock --timeout 30m                # auto-expires after 30 minutes
gitkit lock status                       # show whether a lock is active
gitkit lock status --json                # machine-readable status, see below
gitkit unlock                            # remove the lock
```

Locking twice updates the existing lock (reason, timeout) instead of
stacking or erroring. The `--push` and `--all` flags can be used to add
or modify which operations are locked without removing the existing lock.

## How it works

`gitkit lock` writes a small JSON state file at `.git/gitkit.lock` and
installs `pre-commit` and/or `pre-push` hooks that read it. The hooks are
pure POSIX `sh` — no dependency on the `gitkit` binary — so they stay fast
on every commit and push. A missing, empty, or malformed lock file is always
treated as unlocked: a corrupt lock never blocks an operation.

By default, `gitkit lock` blocks commits only. Use `--push` to add push
blocking, or `--all` to block both. You can call lock multiple times to
add or change which operations are blocked.

If you already had a `pre-commit` or `pre-push` hook, it is backed up to
`pre-commit.gitkit-orig` / `pre-push.gitkit-orig` and chained to — it
still runs after the lock check passes. `gitkit unlock` restores them and
removes the backups.

The lock is per-repository, local only, and never committed or pushed —
it lives entirely under `.git/`.

## Status output

Both `gitkit lock status` (human-readable) and `gitkit lock status --json`
(machine-readable) show per-operation status. This lets you see at a glance
which operations are currently locked:

```
Locked: Agent session
Locked at: 2026-01-01T10:00:00Z
Expires at: 2026-01-01T10:30:00Z
Commit: locked
Push: not locked
```

This shows that commits are blocked, but pushes are allowed.

## Machine-readable status: `lock status --json`

`gitkit lock status --json` emits the same state the human-readable
`gitkit lock status` shows, as a single line of JSON on stdout, so another
program can check whether a repository is locked instead of discovering it
by having an operation rejected. This is a **read-only** surface: gitkit does
not call out to, or know about, whatever consumes it.

```bash
$ gitkit lock status --json
{"active":true,"operations":["commit"],"locked_at":"2026-01-01T00:00:00Z","expires_at":null,"reason":"Agent session","expired":false}
```

Fields, all always present (this key set is a supported contract — do not
rely on a key being renamed or removed without a version bump):

| Key           | Type              | Meaning                                                                 |
|---------------|-------------------|--------------------------------------------------------------------------|
| `active`      | `bool`            | Whether the lock currently blocks the operations it lists — `false` if there is no lock, the lock file is malformed, `operations` is empty, or the lock has expired. |
| `operations`  | `string[]`        | The operations the lock covers. Can be `"commit"`, `"push"`, or both. Empty when there is no lock. |
| `locked_at`   | `string \| null`  | RFC 3339 timestamp the lock was set, or `null` when there is no lock.    |
| `expires_at`  | `string \| null`  | RFC 3339 timestamp the lock expires, or `null` for a lock with no timeout (or no lock at all). |
| `reason`      | `string \| null`  | The `--reason` text, or `null` when there is no lock.                    |
| `expired`     | `bool`            | Whether `expires_at` is in the past, resolved at read time. There is no background process — expiry is only ever checked when something reads the lock. |

A missing or malformed lock file reports the same payload as no lock at
all (`active: false`, every other field `null`/empty) — a corrupt lock
file never blocks a caller, matching the human-readable behavior above.

**Exit code** doubles as the machine-readable signal, so a shell caller can
branch without parsing JSON: `0` when no lock is in force (including an
expired or malformed one), non-zero when one is active. The JSON is still
written to stdout in both cases.

`gitkit lock status --json` works from any directory inside the repository,
the same as the human-readable form.

## File format: `.git/gitkit.lock`

The lock state lives at `.git/gitkit.lock` as a single line of JSON. A
consumer may read this file directly instead of shelling out to
`gitkit lock status --json` — both read the same file, and the schema
below is the supported contract for either path.

```json
{"locked_at":"2026-01-01T00:00:00Z","expires_at":"2026-01-01T00:30:00Z","reason":"Agent session","operations":["commit"]}
```

| Key           | Type              | Meaning                                        |
|---------------|-------------------|-------------------------------------------------|
| `locked_at`   | `string`          | RFC 3339 timestamp the lock was set.             |
| `expires_at`  | `string \| null`  | RFC 3339 timestamp the lock expires, or `null` for no timeout. |
| `reason`      | `string`          | The `--reason` text, or empty string if none was given. |
| `operations`  | `string[]`        | The operations the lock covers.                  |

Notes for a direct reader:

- A missing file means no lock is active.
- Expiry is not enforced by anything in the file itself — a reader must
  compare `expires_at` against the current time itself, the same way
  `gitkit lock status --json` resolves its `expired` field.
- Treat an unparseable file the same as a missing one: unlocked. gitkit's
  own hooks and `status` do the same, so a corrupt file never blocks
  anything on either side.
- This file is local only, lives entirely under `.git/`, and is never
  committed or pushed.

## Limitations: `--no-verify` bypass

Both `git commit --no-verify` and `git push --no-verify` bypass their
respective hooks, including the lock checks. **This is expected and not
treated as a bug.** The lock's threat model is an AI agent following its
instructions, not a human deliberately working around a local safeguard —
so no attempt is made to defend against `--no-verify`. If you need a
guarantee that survives a determined bypass, this is not that guarantee.
