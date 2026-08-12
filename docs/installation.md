---
title: Installation
description: Install gitkit with the quick installer, cargo, or from GitHub Releases.
order: 2
---

# Installation

## Quick install (recommended)

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/UniverLab/gitkit/main/scripts/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/UniverLab/gitkit/main/scripts/install.ps1 | iex
```

## Via cargo

```bash
cargo install gitkit
```

Available on [crates.io](https://crates.io/crates/gitkit).

## GitHub Releases

Precompiled binaries for Linux x86_64, macOS x86_64/ARM64 and Windows
x86_64 are published on the
[Releases](https://github.com/UniverLab/gitkit/releases) page.

## Self-update

gitkit automatically checks GitHub for newer releases each time it runs and
offers to update if a newer version is available. The update replaces the
running binary in place — no need to reinstall or restart your shell between
commands.

### Disable update checks

If you prefer to manage updates yourself, disable the check with:

```bash
export GITKIT_NO_UPDATE_CHECK=1
```

Add this to your shell profile to make it permanent.

### Cargo-installed versions

If gitkit was installed with `cargo install gitkit`, the auto-updater will
detect this and ask you to update using cargo instead:

```bash
cargo install --force gitkit
```

This is because cargo manages the installation and needs to be involved in
the update to maintain consistency.

## Uninstall

**Linux / macOS:**

```bash
rm -f ~/.local/bin/gitkit
rm -rf ~/.gitkit/   # saved builds (optional)
```

**Windows (PowerShell):**

```powershell
Remove-Item "$env:LOCALAPPDATA\gitkit\gitkit.exe" -Force
```
