use anyhow::Result;
use inquire::{MultiSelect, Select, Text};
use std::{collections::HashSet, fs};

use crate::{attributes, builds, config, git, hooks, ignore, utils::find_repo_root};

const BANNER: &str = r#"
           ███   █████    █████       ███   █████   
          ░░░   ░░███    ░░███       ░░░   ░░███    
  ███████ ████  ███████   ░███ █████ ████  ███████  
 ███░░███░░███ ░░░███░    ░███░░███ ░░███ ░░░███░   
░███ ░███ ░███   ░███     ░██████░   ░███   ░███    
░███ ░███ ░███   ░███ ███ ░███░░███  ░███   ░███ ███
░░███████ █████  ░░█████  ████ █████ █████  ░░█████ 
 ░░░░░███░░░░░    ░░░░░  ░░░░ ░░░░░ ░░░░░    ░░░░░  
 ███ ░███                                           
░░██████                                            
 ░░░░░░                                             
"#;

pub fn run() -> Result<()> {
    // Initialize git repository if not already one
    let git_initialized = git::init_if_needed()?;
    if git_initialized {
        println!("  ◇ git repository initialized  ✓");
    }

    println!("{BANNER}");
    println!("  Configure your git repo\n");

    // ── Build selection ─────────────────────────────────────────────────────
    let saved_builds = builds::list_build_names();
    if !saved_builds.is_empty() {
        let mut options = vec!["Start fresh configuration".to_string()];
        options.extend(saved_builds.iter().map(|b| format!("Use build: {b}")));

        let choice = Select::new("Saved builds available", options)
            .with_help_message("↑↓ move  enter confirm  esc start fresh")
            .prompt_skippable()?;

        if let Some(build_name) = choice.as_deref().and_then(|c| c.strip_prefix("Use build: ")) {
            println!();
            let build = builds::load_build(build_name)?;
            builds::apply_build(&build)?;
            println!("\n  Done\n");
            return Ok(());
        }
        println!();
    }

    let cargo_available = std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // ── Hooks ────────────────────────────────────────────────────────────────
    let builtins = hooks::available_builtins();
    let installed_hooks = get_installed_hooks();

    let mut hook_items: Vec<String> = builtins
        .iter()
        .map(|b| {
            let base = format!("{:<25} ({})  —  {}", b.name, b.hook, b.description);
            if installed_hooks.contains(b.name) {
                format!("{} [✓ installed]", base)
            } else {
                base
            }
        })
        .collect();
    hook_items.push("Add custom hook...".to_string());

    let preselected: Vec<usize> = builtins
        .iter()
        .enumerate()
        .filter(|(_, b)| installed_hooks.contains(b.name))
        .map(|(i, _)| i)
        .collect();

    let default_selection = if preselected.is_empty() {
        vec![0usize]
    } else {
        preselected
    };

    let hook_selections = MultiSelect::new("Hooks", hook_items.clone())
        .with_default(&default_selection)
        .with_help_message("↑↓ move  space select  enter confirm  esc skip")
        .prompt_skippable()?
        .unwrap_or_default();

    let mut selected_builtins: Vec<&str> = Vec::new();
    let mut custom_hooks: Vec<(String, String)> = Vec::new();

    for item in &hook_selections {
        if item == "Add custom hook..." {
            let Some(hook_name) = Select::new("Hook type", hooks::valid_hook_names().to_vec())
                .prompt_skippable()?
            else {
                continue;
            };
            let command = Text::new("  Command to run")
                .prompt_skippable()?
                .unwrap_or_default();
            if command.trim().is_empty() {
                continue;
            }
            custom_hooks.push((hook_name.to_string(), command));
        } else if let Some(idx) = hook_items.iter().position(|i| i == item) {
            if idx < builtins.len() {
                selected_builtins.push(builtins[idx].name);
            }
        }
    }

    let hooks_to_remove: Vec<&str> = installed_hooks
        .iter()
        .filter(|h| !selected_builtins.contains(&h.as_str()))
        .map(|s| s.as_str())
        .collect();

    // ── .gitignore ───────────────────────────────────────────────────────────
    println!();
    let all_templates = load_ignore_templates();
    let selected_templates = if all_templates.is_empty() {
        println!("  ⚠  Could not fetch templates (offline?) — skipping .gitignore");
        vec![]
    } else {
        MultiSelect::new(".gitignore templates", all_templates)
            .with_help_message("Type to filter  ↑↓ move  space select  enter confirm  esc skip")
            .with_page_size(10)
            .prompt_skippable()?
            .unwrap_or_default()
    };

    // ── .gitattributes ───────────────────────────────────────────────────────
    println!();
    let attrs_items = vec![
        "line-endings  ★ recommended  —  * text=auto eol=lf",
        "binary-files  —  mark images, PDFs, archives as binary (no diff)",
    ];
    let attrs_keys = ["line-endings", "binary-files"];

    let attrs_selections = MultiSelect::new(".gitattributes", attrs_items.clone())
        .with_default(&[0usize]) // line-endings preselected
        .with_help_message("space select  enter confirm  esc skip")
        .prompt_skippable()?
        .unwrap_or_default();

    let selected_attrs: Vec<&str> = resolve_keys(&attrs_selections, &attrs_items, &attrs_keys);

    // ── Git config ───────────────────────────────────────────────────────────
    println!();
    let configured_keys = get_configured_keys();

    let config_options: Vec<&config::ConfigOption> = config::CONFIG_OPTIONS
        .iter()
        .filter(|o| o.key != "core.pager" || cargo_available)
        .collect();

    let config_labels: Vec<String> = config_options
        .iter()
        .map(|o| {
            if configured_keys.contains(o.key) {
                format!("{} [✓ already set]", o.label)
            } else {
                o.label.to_string()
            }
        })
        .collect();

    let config_labels_refs: Vec<&str> = config_labels.iter().map(|s| s.as_str()).collect();

    let defaults: Vec<usize> = config_options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.recommended || configured_keys.contains(o.key))
        .map(|(i, _)| i)
        .collect();

    let config_selections = MultiSelect::new("Git config", config_labels_refs.clone())
        .with_default(&defaults)
        .with_help_message("↑↓ move  space select  enter confirm  esc skip")
        .prompt_skippable()?
        .unwrap_or_default();

    let selected_config_keys: Vec<&str> = resolve_keys(
        &config_selections,
        &config_labels_refs,
        &config_options.iter().map(|o| o.key).collect::<Vec<_>>(),
    );

    let configs_to_remove: Vec<&str> = config_options
        .iter()
        .filter(|o| configured_keys.contains(o.key) && !selected_config_keys.contains(&o.key))
        .map(|o| o.key)
        .collect();

    // ── Summary & confirm ────────────────────────────────────────────────────
    let has_removals = !hooks_to_remove.is_empty() || !configs_to_remove.is_empty();
    let nothing = selected_builtins.is_empty()
        && custom_hooks.is_empty()
        && selected_templates.is_empty()
        && selected_attrs.is_empty()
        && selected_config_keys.is_empty()
        && !has_removals;

    if nothing {
        println!("\n  Nothing selected — exiting.");
        return Ok(());
    }

    println!("\n  Summary:");
    if !selected_builtins.is_empty() || !custom_hooks.is_empty() {
        let names: Vec<&str> = selected_builtins
            .iter()
            .copied()
            .chain(custom_hooks.iter().map(|(h, _)| h.as_str()))
            .collect();
        println!("  ◆ hooks: {}", names.join(", "));
    }
    if !selected_templates.is_empty() {
        println!("  ◆ .gitignore: {}", selected_templates.join(", "));
    }
    if !selected_attrs.is_empty() {
        println!("  ◆ .gitattributes: {}", selected_attrs.join(", "));
    }
    if !selected_config_keys.is_empty() {
        println!("  ◆ git config: {}", selected_config_keys.join(", "));
    }

    println!();
    let confirmed = inquire::Confirm::new("Apply these changes?")
        .with_default(true)
        .prompt()?;

    if !confirmed {
        println!("  Aborted.");
        return Ok(());
    }

    // ── Apply ────────────────────────────────────────────────────────────────
    println!();
    for name in &selected_builtins {
        hooks::install_builtin(name, false)?;
        println!("  ◇ hook '{name}' installed  ✓");
    }
    for (hook, cmd) in &custom_hooks {
        hooks::install_custom(hook, cmd, false)?;
        println!("  ◇ hook '{hook}' installed  ✓");
    }
    for hook in &hooks_to_remove {
        if let Some(builtin) = hooks::available_builtins().iter().find(|b| b.name == *hook) {
            // Several built-ins can share a hook file (e.g. pre-commit); don't
            // delete the file if a freshly installed selection now owns it.
            let file_reused = selected_builtins.iter().any(|sel| {
                hooks::available_builtins()
                    .iter()
                    .any(|b| b.name == *sel && b.hook == builtin.hook)
            });
            if file_reused {
                continue;
            }
            if hooks::remove_hook(builtin.hook, true).is_ok() {
                println!("  ◇ hook '{hook}' removed  ✓");
            }
        }
    }
    if !selected_templates.is_empty() {
        let joined = selected_templates.join(",");
        ignore::add_templates(&joined, false)?;
        println!("  ◇ .gitignore updated  ✓");
    }
    if !selected_attrs.is_empty() {
        attributes::apply_presets(&selected_attrs)?;
        println!("  ◇ .gitattributes applied  ✓");
    }
    if !selected_config_keys.is_empty() {
        config::apply_config_keys(
            &selected_config_keys,
            cargo_available,
            config::ConfigScope::Local,
        )?;
        println!("  ◇ git config applied  ✓");
    }
    // Only touch the repo's local config; a global value affects every repo,
    // so it is never removed from here.
    for key in &configs_to_remove {
        if config::remove_config_key(key, config::ConfigScope::Local).is_ok() {
            println!("  ◇ git config '{key}' removed  ✓");
        } else {
            println!(
                "  ◇ git config '{key}' is set globally — left untouched (git config --global --unset {key})"
            );
        }
    }

    // ── Save as build ─────────────────────────────────────────────────────
    println!();
    let save_build = inquire::Confirm::new("Save this configuration as a reusable build?")
        .with_default(false)
        .prompt()?;

    if save_build {
        let name = Text::new("  Build name").prompt()?;
        let description = Text::new("  Description (optional)")
            .with_default("")
            .prompt()?;
        let desc_ref = if description.is_empty() {
            None
        } else {
            Some(description.as_str())
        };
        if let Err(e) = builds::save(&name, desc_ref) {
            println!("  ⚠ Failed to save build: {e}");
        }
    }

    println!("\n  Done\n");
    Ok(())
}

fn load_ignore_templates() -> Vec<String> {
    ignore::fetch_template_list().unwrap_or_default()
}

fn get_installed_hooks() -> HashSet<String> {
    let mut installed = HashSet::new();
    if let Ok(root) = find_repo_root() {
        let hooks_dir = root.join(".git").join("hooks");
        if hooks_dir.exists() {
            if let Ok(entries) = fs::read_dir(&hooks_dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if !name.ends_with(".bak") && !name.ends_with(".sample") {
                        let content = fs::read_to_string(entry.path()).unwrap_or_default();
                        if let Some(b) = hooks::detect_builtin(&name, &content) {
                            installed.insert(b.name.to_string());
                        }
                    }
                }
            }
        }
    }
    installed
}

fn get_configured_keys() -> HashSet<String> {
    let mut configured = HashSet::new();

    // Get all config values in one call per scope
    let local_configs = get_all_git_configs("--local");
    let global_configs = get_all_git_configs("--global");

    for option in config::CONFIG_OPTIONS {
        if option.key == "core.pager" {
            continue;
        }
        if let Some(expected_value) = option.value {
            // Check local first, then global
            if local_configs.get(option.key).map(|s| s.as_str()) == Some(expected_value)
                || global_configs.get(option.key).map(|s| s.as_str()) == Some(expected_value)
            {
                configured.insert(option.key.to_string());
            }
        }
    }
    configured
}

fn get_all_git_configs(scope: &str) -> std::collections::HashMap<String, String> {
    let mut configs = std::collections::HashMap::new();

    if let Ok(output) = std::process::Command::new("git")
        .args(["config", scope, "--list"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    configs.insert(key.to_string(), value.to_string());
                }
            }
        }
    }

    configs
}

/// Maps selected display labels back to their corresponding keys.
fn resolve_keys<'a>(
    selections: &[impl AsRef<str>],
    labels: &[&str],
    keys: &[&'a str],
) -> Vec<&'a str> {
    selections
        .iter()
        .filter_map(|item| {
            labels
                .iter()
                .position(|l| *l == item.as_ref())
                .map(|i| keys[i])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_configured_keys_only_returns_known_option_keys() {
        let configured = get_configured_keys();
        for key in &configured {
            assert!(config::CONFIG_OPTIONS.iter().any(|o| o.key == key));
        }
    }

    #[test]
    fn resolve_keys_maps_labels_to_keys() {
        let selections = vec!["option A", "option C"];
        let labels = vec!["option A", "option B", "option C"];
        let keys = vec!["key_a", "key_b", "key_c"];
        let result = resolve_keys(&selections, &labels, &keys);
        assert_eq!(result, vec!["key_a", "key_c"]);
    }
}
