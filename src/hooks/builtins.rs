pub(super) struct Builtin {
    pub name: &'static str,
    pub hook: &'static str,
    pub description: &'static str,
    pub script: &'static str,
}

pub(super) const ALL: &[Builtin] = &[
    Builtin {
        name: "conventional-commits",
        hook: "commit-msg",
        description: "Validates Conventional Commits format",
        script: CONVENTIONAL_COMMITS,
    },
    Builtin {
        name: "no-secrets",
        hook: "pre-commit",
        description: "Detects common secret patterns in staged changes",
        script: NO_SECRETS,
    },
    Builtin {
        name: "branch-naming",
        hook: "pre-commit",
        description: "Validates branch name matches convention",
        script: BRANCH_NAMING,
    },
];

pub(super) fn get(name: &str) -> Option<&'static Builtin> {
    ALL.iter().find(|b| b.name == name)
}

const CONVENTIONAL_COMMITS: &str = r#"#!/bin/sh
commit_msg=$(cat "$1")
pattern='^(feat|fix|docs|style|refactor|perf|test|build|ci|chore|revert)(\(.+\))?: .{1,}'
if ! echo "$commit_msg" | grep -qE "$pattern"; then
  echo "ERROR: Commit message does not follow Conventional Commits format."
  echo "Expected: <type>(<scope>): <description>"
  echo "Types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert"
  exit 1
fi
"#;

const NO_SECRETS: &str = r#"#!/bin/sh
# Detects common secret patterns. Not exhaustive — use dedicated tools for production.
patterns='(AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{35}|ghp_[0-9A-Za-z]{36}|sk-[0-9A-Za-z]{48}|password\s*=\s*["'"'"'][^"'"'"']{8,})'
if git diff --cached --diff-filter=ACM | grep -qE "$patterns"; then
  echo "ERROR: Possible secret detected in staged changes."
  echo "Review your changes and remove any credentials before committing."
  exit 1
fi
"#;

const BRANCH_NAMING: &str = r#"#!/bin/sh
branch=$(git symbolic-ref --short HEAD)
pattern='^(main|master|develop|release/.+|hotfix/.+|feat/.+|fix/.+|chore/.+)$'
if ! echo "$branch" | grep -qE "$pattern"; then
  echo "ERROR: Branch name '$branch' does not match naming convention."
  echo "Expected pattern: main|master|develop|release/*|hotfix/*|feat/*|fix/*|chore/*"
  exit 1
fi
"#;
