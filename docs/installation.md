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
