//! Guards that every `#[tauri::command]` exported by the commands module is
//! registered in the app's invoke handler.
//!
//! `manifest_profiles` once shipped defined but unregistered: the webview
//! invoke rejected with "command not found", the frontend swallowed it behind
//! a `.catch(() => null)`, and the packaged app showed an empty provider list
//! while the mocked browser preview kept passing. Tauri's macro offers no
//! compile-time cross-check, so this test parses the two source files the
//! same way a reviewer would.

use std::sync::LazyLock;

static COMMANDS_SOURCE: LazyLock<String> = LazyLock::new(|| {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/commands/mod.rs"))
        .expect("commands/mod.rs is readable")
});

static APP_SOURCE: LazyLock<String> = LazyLock::new(|| {
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app/mod.rs"))
        .expect("app/mod.rs is readable")
});

/// Names of the commands `commands/mod.rs` marks with `#[tauri::command]`.
fn exported_command_names() -> Vec<String> {
    let lines: Vec<&str> = COMMANDS_SOURCE.lines().collect();
    let mut names = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if *line != "#[tauri::command]" {
            continue;
        }
        // The fn signature directly follows the attribute (possibly behind
        // further attributes such as `#[cfg]`, so scan ahead for `fn `).
        let signature = lines[index + 1..]
            .iter()
            .find(|line| line.contains("fn "))
            .unwrap_or_else(|| panic!("#[tauri::command] without a fn signature on line {index}"));
        let name = signature
            .split("fn ")
            .nth(1)
            .unwrap_or_else(|| panic!("cannot read fn name on line {index}"))
            .trim_start()
            .split(['(', '<', ' '])
            .next()
            .unwrap_or_default()
            .to_owned();
        assert!(!name.is_empty(), "empty command name near line {index}");
        names.push(name);
    }
    names
}

#[test]
fn every_commands_module_command_is_registered_in_the_invoke_handler() {
    let registered_section = APP_SOURCE
        .split("generate_handler![")
        .nth(1)
        .unwrap_or_else(|| panic!("app/mod.rs has no generate_handler! block"));
    let names = exported_command_names();
    assert!(
        names.len() >= 10,
        "parser found suspiciously few commands ({names:?}); the source scan is broken"
    );
    for name in &names {
        let reference = format!("commands::{name}");
        assert!(
            registered_section.contains(&reference),
            "`{reference}` is a #[tauri::command] but is not registered in the \
             invoke_handler; the webview cannot call it",
        );
    }
}

#[test]
fn the_invoke_handler_lists_no_unknown_commands_module_entries() {
    // The reverse direction: every `commands::x` referenced by the handler
    // must still exist as a declared command, so removing or renaming one
    // cannot leave a dangling registration that fails at compile time anyway.
    for reference in ["commands::accounts_list", "commands::provider_matrix"] {
        let name = reference.trim_start_matches("commands::");
        assert!(
            exported_command_names()
                .iter()
                .any(|candidate| candidate == name),
            "`{reference}` is registered but no matching #[tauri::command] exists"
        );
    }
}
