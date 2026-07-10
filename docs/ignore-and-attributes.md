---
title: Ignore & Attributes
description: Generate .gitignore from templates and apply .gitattributes presets.
order: 5
---

# Ignore & Attributes

## `.gitignore`

Gitkit generates and merges `.gitignore` files from
[gitignore.io](https://www.toptal.com/developers/gitignore) templates plus
its own built-ins (like `agentic` for AI coding agents):

```bash
gitkit ignore add rust,vscode,agentic   # generate/merge templates
gitkit ignore list                      # list available templates
gitkit ignore list python               # filter the list
```

Existing patterns are merged, not overwritten — your manual entries
survive.

## `.gitattributes`

```bash
gitkit attributes init
```

Applies a line-endings preset (`* text=auto eol=lf`) and binary-file
attributes so checkouts behave identically across Linux, macOS and
Windows.

Both files can also be configured interactively from the `gitkit` wizard,
which offers a filterable search across all templates.
