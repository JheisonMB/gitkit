---
title: Hooks
description: Built-in hooks (conventional commits, AI trailer rejection, secret detection, branch naming, invisible Unicode detection) and custom shell commands.
order: 4
---

# Hooks

## Built-in hooks

Built-ins are embedded in the binary — no network required.

| Name | Hook | Description |
|---|---|---|
| `conventional-commits` | `commit-msg` | Validates Conventional Commits format |
| `no-trailers` | `commit-msg` | Rejects commit messages carrying AI attribution trailers |
| `no-secrets` | `pre-commit` | Detects common secret patterns in staged changes |
| `branch-naming` | `pre-commit` | Validates branch name matches convention |
| `no-invisibles` | `pre-commit` | Rejects added lines carrying invisible Unicode characters |

```bash
gitkit hooks list --available   # see all built-ins with descriptions
gitkit hooks add no-secrets     # install one (hook type inferred)
```

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
