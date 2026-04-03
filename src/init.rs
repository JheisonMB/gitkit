use anyhow::Result;
use inquire::{MultiSelect, Text};

use crate::{attributes, config, hooks, ignore};

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
    println!("{BANNER}");
    println!("  Configure your git repo\n");

    let cargo_available = std::process::Command::new("cargo")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // ── Hooks ────────────────────────────────────────────────────────────────
    let builtins = hooks::available_builtins();
    let mut hook_items: Vec<String> = builtins
        .iter()
        .map(|b| format!("{:<25} ({})  —  {}", b.name, b.hook, b.description))
        .collect();
    hook_items.push("Add custom hook...".to_string());

    let hook_selections = MultiSelect::new("Hooks", hook_items.clone())
        .with_default(&[0usize]) // conventional-commits preselected
        .with_help_message("↑↓ move  space select  enter confirm  esc skip")
        .prompt_skippable()?
        .unwrap_or_default();

    let mut selected_builtins: Vec<&str> = Vec::new();
    let mut custom_hooks: Vec<(String, String)> = Vec::new();

    for item in &hook_selections {
        if item == "Add custom hook..." {
            let valid = hooks::valid_hook_names().join(", ");
            let hook_name = Text::new("  Hook name")
                .with_help_message(&format!("Valid: {valid}"))
                .prompt()?;
            let command = Text::new("  Command to run").prompt()?;
            custom_hooks.push((hook_name, command));
        } else if let Some(idx) = hook_items.iter().position(|i| i == item) {
            if idx < builtins.len() {
                selected_builtins.push(builtins[idx].name);
            }
        }
    }

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
    let config_options: Vec<&config::ConfigOption> = config::CONFIG_OPTIONS
        .iter()
        .filter(|o| o.key != "core.pager" || cargo_available)
        .collect();

    let config_labels: Vec<&str> = config_options.iter().map(|o| o.label).collect();

    // pre-select recommended ones
    let defaults: Vec<usize> = config_options
        .iter()
        .enumerate()
        .filter(|(_, o)| o.recommended)
        .map(|(i, _)| i)
        .collect();

    let config_selections = MultiSelect::new("Git config", config_labels.clone())
        .with_default(&defaults)
        .with_help_message("↑↓ move  space select  enter confirm  esc skip")
        .prompt_skippable()?
        .unwrap_or_default();

    let selected_config_keys: Vec<&str> = resolve_keys(
        &config_selections,
        &config_labels,
        &config_options.iter().map(|o| o.key).collect::<Vec<_>>(),
    );

    // ── Summary & confirm ────────────────────────────────────────────────────
    let nothing = selected_builtins.is_empty()
        && custom_hooks.is_empty()
        && selected_templates.is_empty()
        && selected_attrs.is_empty()
        && selected_config_keys.is_empty();

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
        config::apply_config_keys(&selected_config_keys, cargo_available)?;
        println!("  ◇ git config applied  ✓");
    }

    println!("\n  Done\n");
    Ok(())
}

fn load_ignore_templates() -> Vec<String> {
    ignore::fetch_template_list().unwrap_or_default()
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
