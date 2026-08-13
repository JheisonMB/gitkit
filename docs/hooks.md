---
title: Hooks
description: Built-in hooks (conventional commits, no-body messages, AI trailer rejection, secret detection, branch naming, invisible Unicode detection) and custom shell commands.
order: 4
---

# Hooks

## Built-in hooks

Built-ins are embedded in the binary — no network required.

| Name | Hook | Description |
|---|---|---|
| `conventional-commits` | `commit-msg` | Validates Conventional Commits format |
| `no-body` | `commit-msg` | Rejects a commit message that has a body |
| `no-trailers` | `commit-msg` | Rejects commit messages carrying AI attribution trailers |
| `no-secrets` | `pre-commit` | Detects common secret patterns in staged changes |
| `branch-naming` | `pre-commit` | Validates branch name matches convention |
| `no-invisibles` | `pre-commit` | Rejects added lines carrying invisible Unicode characters |

```bash
gitkit hooks list --available   # see all built-ins with descriptions
gitkit hooks add no-secrets     # install one (hook type inferred)
```

### `conventional-commits`

Validates that the **subject line** (the first line only — a conventional-looking
line further down the message doesn't count) matches
`<type>(<scope>): <description>`, where `<type>` is one of `feat`, `fix`,
`docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore` or
`revert`, and `<scope>` is optional. A breaking change can be marked with a
`!` before the colon, with or without a scope: `feat!: drop the v1 endpoint`
or `feat(api)!: drop the v1 endpoint`.

### `no-body`

Rejects a commit message that has a body. A conforming message is one line,
with any number of trailing newlines. The subject line already says what
changed; a bulleted restatement of the diff adds nothing `git show` can't
show better, and it pushes the one thing prose is good at — *why* — out of
the message. That narrative belongs in the pull request description, not
the commit body.

The only body this hook accepts is a **breaking-change footer**: a blank
line followed by `BREAKING CHANGE:` or `BREAKING-CHANGE:` (uppercase, per
the Conventional Commits spec — a lowercase `breaking change:` is not
recognized) and nothing else. The footer's own description may wrap onto
continuation lines, but a paragraph before it or a bullet list after it is
rejected — the exception is for stating a breaking change, not a loophole
for arbitrary bodies. For a breaking change that fits in the subject, use
`conventional-commits`'s `!` form instead and skip the body entirely.

Revert (`Revert "..."`), merge (`Merge branch '...'`), `fixup!` and
`squash!` commit messages are auto-generated, not hand-authored, and are
always accepted regardless of body.

### `no-trailers`

Rejects a commit whose message contains a `Co-Authored-By:`, `Assisted-By:`
or `AI-Assisted-By:` line naming a known AI vendor no-reply address (at
minimum `noreply@anthropic.com`), a `Claude-Session:` line, or a "Generated
with" line. Genuine human `Co-Authored-By:` trailers are left untouched — a
rule in a prompt is advisory, this hook is not. The commit is refused with
the offending line and its line number; it never rewrites your message.

### `no-invisibles`

Rejects a commit that **adds** a line containing an invisible Unicode
character: zero-width characters (ZWSP, ZWNJ, ZWJ, word joiner, a
mid-string BOM, Mongolian vowel separator), bidirectional control
characters (which also enable ["Trojan Source"](https://trojansource.codes/)
attacks, where displayed code order differs from compiled order), or
Unicode tag characters (U+E0000–U+E007F, which have no rendering at all).
This is the class of character used to carry provenance marks into pasted
text, and the class that survives copy-paste into a repository unnoticed.

This hook finds **invisible characters**, not watermarks in general. Some
text watermarks are carried in the *choice* of ordinary words rather than
in any extra character — those are undetectable by inspecting codepoints,
and this hook makes no claim about them.

**Scope: only lines this commit adds.** The staged diff is read directly
(via `git diff --cached`); lines the commit doesn't touch are never
scanned, even if they carry an invisible character from years ago. Touching
one line in a large file should not block your commit over someone else's
character on a line you didn't write. A repository-wide sweep for
pre-existing invisible characters is a deliberate, separate action, not
something a commit hook should ambush you with. Renamed files are handled
the same way: git's rename detection (`-M`) means a pure rename with no
content change produces no scannable lines, and a rename that also edits
content only exposes the lines that actually changed.

Two characters are deliberately **out of scope** even though they can look
invisible: variation selectors (e.g. U+FE0F, which selects the emoji
presentation of the preceding character — stripping it changes the
rendered glyph, and in something like `derive_key("🔑🛡️")` it would change
the derived key) and NBSP/soft hyphen (both have legitimate uses in prose
and typesetting). A byte-order mark as the very first character of a file
is not flagged either; a `U+FEFF` anywhere else in the file is.

U+200D ZERO WIDTH JOINER is flagged unconditionally, including inside
legitimate multi-person emoji sequences — telling a "load-bearing" ZWJ
apart from a smuggled one would need an emoji-sequence table this hook
doesn't carry, so it accepts that false positive rather than risk missing
a real one.

The commit is refused with each occurrence's file, line, column and
codepoint (e.g. `README.md:2:7: U+200B ZERO WIDTH SPACE`) — the character
can't be found by eye, so the report has to say exactly where it is. Like
`no-secrets`, it only ever rejects; it never rewrites your files.

Unlike the other built-ins, the installed hook execs back into `gitkit`
itself (`gitkit hooks scan-invisibles`) rather than doing the check in
`sh` — correct codepoint and column reporting needs real Unicode-aware
text handling.

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

## Hook health

A hook file that exists is not the same as a hook that runs. Git silently
ignores a hook file that isn't marked executable — it doesn't error, it just
never fires, and the only sign is a warning
(`hook was ignored because it's not set as executable`) that's easy to miss
in commit scrollback. `gitkit status` reports each installed hook's actual
health, not just its presence:

```bash
gitkit status
```

```
Hooks:
  ✓ conventional-commits (commit-msg) — active
  ✗ no-secrets (pre-commit) — dormant: not executable, so git ignores it and never runs it (fix with `gitkit status --repair`)
  ~ pre-push — modified: "cargo test"
```

- **active** — installed, executable, matches a built-in verbatim. Git runs it.
- **dormant** — installed and matches a built-in verbatim, but is not
  executable. **Git ignores it.** This is the state a broken install or a
  lost executable bit leaves behind.
- **modified** — installed, but its content doesn't match any built-in
  (a hand-edited built-in, or a custom command installed with
  `gitkit hooks add <hook> "<command>"`). Not an error — gitkit never
  touches it.

A hook with no file at all simply doesn't appear in the list.

```bash
gitkit status --repair   # sets the executable bit on every dormant hook
gitkit status --strict   # exits non-zero if any hook is dormant (for CI)
```

`--repair` only sets the executable bit on hooks classified `dormant` — the
content already matches a built-in verbatim, so there's nothing to rewrite.
It never touches a `modified` hook (that might be a deliberate edit) and
never installs a hook that isn't there at all (that's what `hooks add` is
for). A bare `gitkit status` never modifies anything; `--repair` is always
opt-in.

On Windows the executable bit doesn't exist, so a hook there is never
reported `dormant`.

## Machine-wide status

`gitkit status` only looks at the current repository. If a hook goes dormant
in a repo you aren't currently sitting in, nothing tells you — that's how a
hook can silently stop running for months.

`gitkit status --global` answers "which repositories on this machine has
gitkit touched, and are they healthy?" in one screen:

```bash
gitkit status --global
```

Every time `hooks add`, `init`, `config`, `ignore`, `attributes` or
`build apply` touches a repository, gitkit records its absolute path in
`~/.gitkit/registry.toml` — alongside a timestamp and what was applied.
That registry only ever supplies *where to look*. `--global` re-reads every
hook's health straight from disk at query time, using the same
active/dormant/modified/absent states as a local `gitkit status`; it never
trusts the registry's own record of what was installed. Delete a hook by
hand, or delete the whole repository, and `--global` reports exactly that —
`absent` or `gone` — instead of repeating a stale claim.

```bash
gitkit status --global --prune   # also drop registry entries for repos that no longer exist
```

A bare `--global` never modifies anything; pruning is opt-in.

Repositories configured before the registry existed aren't in it yet.
Adopt them with an explicit scan:

```bash
gitkit status --scan ~/Projects   # find repos with gitkit hooks under a directory and register them
```

`--scan` never runs implicitly and never defaults to `$HOME` — you always
name the directory. It skips noisy directories (`node_modules`, `target`,
`.cargo`, ...) and never follows symlinks out of the directory you gave it.
