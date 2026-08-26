use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::builtins;
use crate::utils::find_repo_root;

const RULES_FILE: &str = ".gitmessage-rules.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct MessageRule {
    pub name: String,
    pub pattern: String,
    pub direction: Direction,
    pub scope: Scope,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Direction {
    MustMatch,
    MustNotMatch,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Scope {
    Subject,
    WholeMessage,
}

fn rules_path() -> Result<PathBuf> {
    let root = find_repo_root().context("not inside a git repository")?;
    Ok(root.join(RULES_FILE))
}

pub(crate) fn load_rules() -> Result<Vec<MessageRule>> {
    let path = match rules_path() {
        Ok(p) => p,
        Err(_) => return Ok(Vec::new()),
    };

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read rules file: {}", path.display()))?;

    if content.trim().is_empty() {
        return Ok(Vec::new());
    }

    let rules: Vec<MessageRule> = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse rules file: {}", path.display()))?;
    Ok(rules)
}

pub(crate) fn validate_rules() -> Result<Vec<MessageRule>> {
    let rules = load_rules()?;
    anyhow::ensure!(
        !rules.is_empty(),
        "no message-rules configured — create '{}' with at least one rule \
         before running 'gitkit hooks add message-rules'",
        RULES_FILE
    );
    for rule in &rules {
        regex::Regex::new(&rule.pattern).with_context(|| {
            format!(
                "rule '{}': pattern '{}' is not a valid regex",
                rule.name, rule.pattern
            )
        })?;
    }
    Ok(rules)
}

pub(crate) fn generate_script() -> String {
    r#"#!/bin/sh
# gitkit-builtin: message-rules
# Validates the commit message against user-defined rules from .gitmessage-rules.json.
# Delegates to gitkit itself for the actual regex evaluation, so patterns use
# the same Rust regex engine that was validated at install time.
exec gitkit hooks scan-message-rules "$1"
"#
    .to_string()
}

/// Pure rule-checking logic: iterate rules, regex match by scope, collect failures.
/// Shared by both `evaluate_message` (production) and tests.
pub(crate) fn check_rules(rules: &[MessageRule], full_message: &str) -> Vec<String> {
    let subject = full_message.lines().next().unwrap_or("");

    if builtins::is_auto_generated_message(subject) {
        return Vec::new();
    }

    let mut failures: Vec<String> = Vec::new();

    for rule in rules {
        let re = match regex::Regex::new(&rule.pattern) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let text = match rule.scope {
            Scope::Subject => subject,
            Scope::WholeMessage => full_message,
        };

        let matched = re.is_match(text);

        match rule.direction {
            Direction::MustMatch => {
                if !matched {
                    failures.push(format!("ERROR: [{}] {}", rule.name, rule.message));
                }
            }
            Direction::MustNotMatch => {
                if matched {
                    failures.push(format!("ERROR: [{}] {}", rule.name, rule.message));
                }
            }
        }
    }

    failures
}

pub(crate) fn evaluate_message(msg_file: &Path) -> Result<()> {
    let rules = load_rules()?;
    let full_message = std::fs::read_to_string(msg_file)
        .with_context(|| format!("failed to read commit message file: {}", msg_file.display()))?;

    let failures = check_rules(&rules, &full_message);

    if failures.is_empty() {
        Ok(())
    } else {
        for f in &failures {
            eprintln!("{f}");
        }
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rule(
        name: &str,
        pattern: &str,
        direction: Direction,
        scope: Scope,
        message: &str,
    ) -> MessageRule {
        MessageRule {
            name: name.to_string(),
            pattern: pattern.to_string(),
            direction,
            scope,
            message: message.to_string(),
        }
    }

    fn evaluate_rules(rules: &[MessageRule], message: &str) -> (bool, Vec<String>) {
        let failures = check_rules(rules, message);
        if failures.is_empty() {
            (true, failures)
        } else {
            (false, failures)
        }
    }

    #[test]
    fn positive_must_match_rule_accepts_matching_subject() {
        let rules = vec![make_rule(
            "jira-prefix",
            r"^[A-Z]+-\d+",
            Direction::MustMatch,
            Scope::Subject,
            "Subject must start with a JIRA ticket prefix",
        )];
        let (accepted, failures) = evaluate_rules(&rules, "PROJ-123 fix the thing\n");
        assert!(accepted, "expected acceptance: {failures:?}");
    }

    #[test]
    fn positive_must_match_rule_rejects_non_matching_subject() {
        let rules = vec![make_rule(
            "jira-prefix",
            r"^[A-Z]+-\d+",
            Direction::MustMatch,
            Scope::Subject,
            "Subject must start with a JIRA ticket prefix",
        )];
        let (accepted, failures) = evaluate_rules(&rules, "fix the thing\n");
        assert!(!accepted, "expected rejection");
        assert!(
            failures[0].contains("jira-prefix"),
            "failures: {failures:?}"
        );
        assert!(
            failures[0].contains("JIRA ticket prefix"),
            "failures: {failures:?}"
        );
    }

    #[test]
    fn negative_must_not_match_rule_accepts_clean_subject() {
        let rules = vec![make_rule(
            "no-trailer-in-subject",
            "Co-Authored-By:",
            Direction::MustNotMatch,
            Scope::Subject,
            "Subject must not contain trailer lines",
        )];
        let (accepted, failures) = evaluate_rules(&rules, "fix: the thing\n");
        assert!(accepted, "expected acceptance: {failures:?}");
    }

    #[test]
    fn negative_must_not_match_rule_rejects_matching_subject() {
        let rules = vec![make_rule(
            "no-trailer-in-subject",
            "Co-Authored-By:",
            Direction::MustNotMatch,
            Scope::Subject,
            "Subject must not contain trailer lines",
        )];
        let (accepted, failures) = evaluate_rules(&rules, "fix: the thing; Co-Authored-By: bot\n");
        assert!(!accepted, "expected rejection");
        assert!(
            failures[0].contains("no-trailer-in-subject"),
            "failures: {failures:?}"
        );
    }

    #[test]
    fn subject_scope_does_not_fire_on_body_only_match() {
        let rules = vec![make_rule(
            "no-trailer-in-subject",
            "Co-Authored-By:",
            Direction::MustNotMatch,
            Scope::Subject,
            "Subject must not contain trailer lines",
        )];
        let (accepted, failures) = evaluate_rules(
            &rules,
            "fix: the thing\n\nCo-Authored-By: bot <bot@example.com>\n",
        );
        assert!(
            accepted,
            "a subject-scoped rule must not fire on a body-only match: {failures:?}"
        );
    }

    #[test]
    fn whole_message_scope_fires_on_body_match() {
        let rules = vec![make_rule(
            "no-ai-trailer",
            "noreply@anthropic\\.com",
            Direction::MustNotMatch,
            Scope::WholeMessage,
            "Message must not contain AI vendor attribution",
        )];
        let (accepted, failures) = evaluate_rules(
            &rules,
            "fix: the thing\n\nCo-Authored-By: Claude <noreply@anthropic.com>\n",
        );
        assert!(
            !accepted,
            "a whole-message rule must fire on a body match: {failures:?}"
        );
    }

    #[test]
    fn whole_message_scope_includes_subject_line() {
        let rules = vec![make_rule(
            "body-keyword",
            "BREAKING",
            Direction::MustMatch,
            Scope::WholeMessage,
            "Message body must mention BREAKING",
        )];
        let (accepted, _failures) = evaluate_rules(&rules, "BREAKING: fix the thing\n");
        assert!(accepted, "whole-message scope includes the subject line");
    }

    #[test]
    fn multiple_failing_rules_all_reported() {
        let rules = vec![
            make_rule(
                "jira-prefix",
                r"^[A-Z]+-\d+",
                Direction::MustMatch,
                Scope::Subject,
                "Subject must start with a JIRA ticket prefix",
            ),
            make_rule(
                "min-length",
                r".{15,}",
                Direction::MustMatch,
                Scope::Subject,
                "Subject must be at least 15 characters",
            ),
        ];
        let (accepted, failures) = evaluate_rules(&rules, "fix: short\n");
        assert!(!accepted);
        assert!(
            failures.iter().any(|f| f.contains("jira-prefix")),
            "failures: {failures:?}"
        );
        assert!(
            failures.iter().any(|f| f.contains("min-length")),
            "failures: {failures:?}"
        );
    }

    #[test]
    #[allow(clippy::invalid_regex)]
    fn uncompilable_pattern_rejected_at_validation() {
        let result = regex::Regex::new("[invalid");
        assert!(result.is_err(), "an unclosed bracket must not compile");
    }

    #[test]
    fn generated_script_starts_with_shebang_and_marker() {
        let script = generate_script();
        assert!(script.starts_with("#!/bin/sh"));
        assert!(script.contains("# gitkit-builtin: message-rules"));
    }

    #[test]
    fn generated_script_execs_into_gitkit() {
        let script = generate_script();
        assert!(
            script.contains("exec gitkit hooks scan-message-rules"),
            "the installed hook must exec back into gitkit for real regex evaluation: {script}"
        );
    }

    #[test]
    fn exempt_revert_messages() {
        let rules = vec![make_rule(
            "jira-prefix",
            r"^[A-Z]+-\d+",
            Direction::MustMatch,
            Scope::Subject,
            "Subject must start with a JIRA ticket prefix",
        )];
        let (accepted, _) = evaluate_rules(
            &rules,
            "Revert \"feat(x): add thing\"\n\nThis reverts commit abc123.\n",
        );
        assert!(accepted, "revert messages must be exempt");
    }

    #[test]
    fn exempt_merge_messages() {
        let rules = vec![make_rule(
            "jira-prefix",
            r"^[A-Z]+-\d+",
            Direction::MustMatch,
            Scope::Subject,
            "Subject must start with a JIRA ticket prefix",
        )];
        let (accepted, _) = evaluate_rules(&rules, "Merge branch 'develop' into main\n");
        assert!(accepted, "merge messages must be exempt");
    }

    #[test]
    fn exempt_fixup_messages() {
        let rules = vec![make_rule(
            "jira-prefix",
            r"^[A-Z]+-\d+",
            Direction::MustMatch,
            Scope::Subject,
            "Subject must start with a JIRA ticket prefix",
        )];
        let (accepted, _) = evaluate_rules(&rules, "fixup! feat(x): add thing\n");
        assert!(accepted, "fixup messages must be exempt");
    }

    #[test]
    fn exempt_squash_messages() {
        let rules = vec![make_rule(
            "jira-prefix",
            r"^[A-Z]+-\d+",
            Direction::MustMatch,
            Scope::Subject,
            "Subject must start with a JIRA ticket prefix",
        )];
        let (accepted, _) = evaluate_rules(&rules, "squash! feat(x): add thing\n");
        assert!(accepted, "squash messages must be exempt");
    }

    #[test]
    fn whole_message_anchor_semantics_match_rust_regex() {
        let rules = vec![make_rule(
            "must-start-with-breaking",
            r"^BREAKING",
            Direction::MustMatch,
            Scope::WholeMessage,
            "Message must start with BREAKING",
        )];
        let (accepted, _) = evaluate_rules(&rules, "not breaking\n\nBREAKING: smuggled in later\n");
        assert!(
            !accepted,
            "^ under Rust regex must anchor to start of whole message, not match any line"
        );
    }

    #[test]
    fn message_rules_exemption_covers_all_shared_prefixes() {
        for prefix in builtins::AUTO_GENERATED_MESSAGE_PREFIXES {
            let msg = format!("{prefix}something\n");
            let rules = vec![make_rule(
                "jira-prefix",
                r"^[A-Z]+-\d+",
                Direction::MustMatch,
                Scope::Subject,
                "Subject must start with a JIRA ticket prefix",
            )];
            let (accepted, _) = evaluate_rules(&rules, &msg);
            assert!(
                accepted,
                "prefix '{prefix}' must be exempt — AUTO_GENERATED_MESSAGE_PREFIXES and \
                 is_auto_generated_message must stay in sync"
            );
        }
    }

    #[test]
    #[serial_test::serial]
    fn validate_rules_rejects_when_no_rules_configured() {
        let dir = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let result = validate_rules();
        assert!(
            result.is_err(),
            "validate_rules must refuse when no rules are configured"
        );
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("no message-rules configured"),
            "error must name the missing config: {err_msg}"
        );

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[test]
    #[serial_test::serial]
    fn validate_rules_rejects_uncompilable_pattern() {
        let dir = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let bad_rule = MessageRule {
            name: "bad".to_string(),
            pattern: "[invalid".to_string(),
            direction: Direction::MustMatch,
            scope: Scope::Subject,
            message: "bad pattern".to_string(),
        };
        let json = serde_json::to_string_pretty(&[bad_rule]).unwrap();
        std::fs::write(dir.path().join(".gitmessage-rules.json"), json).unwrap();

        let result = validate_rules();
        assert!(result.is_err(), "uncompilable pattern must be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("bad"),
            "error must name the rule: {err_msg}"
        );

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[test]
    #[serial_test::serial]
    fn load_rules_returns_empty_when_file_not_present() {
        let dir = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let rules = load_rules().unwrap();
        assert!(
            rules.is_empty(),
            "no rules should be returned when file is not present"
        );

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[test]
    #[serial_test::serial]
    fn load_rules_parses_json_from_tracked_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        let original = std::env::current_dir().ok();
        let _ = std::env::set_current_dir(dir.path());

        let rule = MessageRule {
            name: "jira-prefix".to_string(),
            pattern: r"^[A-Z]+-\d+".to_string(),
            direction: Direction::MustMatch,
            scope: Scope::Subject,
            message: "JIRA prefix required".to_string(),
        };
        let json = serde_json::to_string_pretty(&[rule]).unwrap();
        std::fs::write(dir.path().join(".gitmessage-rules.json"), json).unwrap();

        let rules = load_rules().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "jira-prefix");

        if let Some(orig) = original {
            let _ = std::env::set_current_dir(orig);
        }
    }

    #[test]
    fn rules_file_path_is_tracked_constant() {
        assert_eq!(RULES_FILE, ".gitmessage-rules.json");
    }

    #[test]
    fn rule_serialization_roundtrip() {
        let rule = MessageRule {
            name: "test".to_string(),
            pattern: ".*".to_string(),
            direction: Direction::MustNotMatch,
            scope: Scope::WholeMessage,
            message: "test message".to_string(),
        };
        let json = serde_json::to_string(std::slice::from_ref(&rule)).unwrap();
        let parsed: Vec<MessageRule> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], rule);
    }
}
