#!/usr/bin/env bash
# Demo scenario for gitkit — replayed inside asciinema by record.sh.
# Simulates human typing, then runs the real command.

set -e

PROMPT='\033[1;32m❯\033[0m '

type_cmd() {
    printf "$PROMPT"
    for ((i = 0; i < ${#1}; i++)); do
        printf '%s' "${1:i:1}"
        sleep 0.04
    done
    sleep 0.4
    printf '\n'
}

run() {
    type_cmd "$1"
    eval "$1"
    sleep "${2:-1.2}"
}

WORKDIR="$(mktemp -d)"
cd "$WORKDIR"
trap 'rm -rf "$WORKDIR"' EXIT

mkdir my-project && cd my-project
git init -q

sleep 0.6
run "gitkit status" 1.5
run "gitkit hooks list --available" 1.8
run "gitkit hooks add conventional-commits" 1.5
run "gitkit hooks add no-secrets" 1.5
run "gitkit ignore add rust,visualstudiocode" 1.5
run "gitkit attributes init" 1.5
run "gitkit status" 3
sleep 1
