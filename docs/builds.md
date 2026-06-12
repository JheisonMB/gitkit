---
title: Builds
description: Save a repo configuration once and re-apply it to any project.
order: 7
---

# Builds

A **build** is a saved snapshot of a repository's gitkit configuration —
hooks, ignore templates, attributes and config presets — stored as a TOML
file under `~/.gitkit/builds/`.

Builds turn your preferred setup into a one-liner for every future
project.

## Workflow

```bash
# Configure a repo the way you like, then save it
gitkit build save rust-dev --description "Rust development setup"

# Apply it to any other project
cd /path/to/other/project
gitkit build apply rust-dev
```

## Managing builds

```bash
gitkit build list             # list saved builds
gitkit build save <name>      # save current repo config as a build
gitkit build apply <name>     # apply a saved build
gitkit build delete <name>    # delete a saved build
```

Because builds are plain TOML files, you can version them in your dotfiles
and share them across machines.
