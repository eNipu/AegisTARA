//! The workbench command table (WBX-11): unique ids, translated labels and
//! tooltips, no clashing keys, and platform shortcut labels.

use std::collections::HashSet;

use aegis_client::command_table::{command, commands, shortcut_label};
use aegis_client::i18n::{Catalog, Language};

#[test]
fn ids_are_unique_and_icons_are_kebab() {
    let mut seen = HashSet::new();
    for c in commands() {
        assert!(seen.insert(c.id), "duplicate id {}", c.id);
        assert!(!c.icon.is_empty(), "{} has no icon", c.id);
        assert!(
            c.icon
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'),
            "{} icon {} is not lower-case kebab",
            c.id,
            c.icon
        );
        assert!(!c.icon.starts_with('-') && !c.icon.ends_with('-'));
        assert_eq!(command(c.id), Some(c));
    }
    assert_eq!(commands().len(), 26);
    assert_eq!(command("no-such-command"), None);
}

#[test]
fn every_label_and_tooltip_is_translated() {
    for language in [Language::En, Language::Ja, Language::Pseudo] {
        let catalog = Catalog::load(language);
        for c in commands() {
            let label = catalog.tr(&c.label_id());
            let tip = catalog.tr(&c.tooltip_id());
            assert!(
                !label.starts_with('['),
                "{language:?} misses {}",
                c.label_id()
            );
            assert!(
                !tip.starts_with('['),
                "{language:?} misses {}",
                c.tooltip_id()
            );
            assert_ne!(label, tip, "{language:?} {}: tooltip repeats label", c.id);
        }
    }
}

#[test]
fn no_two_commands_share_keys_in_a_context() {
    for mac in [true, false] {
        let mut seen = HashSet::new();
        for c in commands() {
            if let Some(keys) = c.keys_for(mac) {
                assert!(
                    seen.insert((c.context, keys.clone())),
                    "{} reuses {keys} (mac {mac})",
                    c.id
                );
            }
        }
    }
}

fn keys_off_mac(id: &str) -> String {
    command(id)
        .and_then(|c| c.keys_for(false))
        .expect("command with keys")
}

#[test]
fn shortcut_labels_per_platform() {
    assert_eq!(shortcut_label("cmd-k", true), "⌘K");
    assert_eq!(shortcut_label("cmd-shift-z", true), "⇧⌘Z");
    assert_eq!(shortcut_label("cmd-alt-b", true), "⌥⌘B");
    assert_eq!(shortcut_label("ctrl-tab", true), "⌃Tab");
    assert_eq!(shortcut_label("enter", true), "Return");
    assert_eq!(shortcut_label("enter", false), "Enter");
    assert_eq!(shortcut_label("down", true), "↓");
    assert_eq!(shortcut_label("cmd-/", true), "⌘/");
    assert_eq!(shortcut_label(&keys_off_mac("palette"), false), "Ctrl+K");
    assert_eq!(shortcut_label(&keys_off_mac("redo"), false), "Ctrl+Y");
    assert_eq!(shortcut_label("ctrl-shift-tab", false), "Ctrl+Shift+Tab");
    assert_eq!(
        shortcut_label(&keys_off_mac("toggle-inspector"), false),
        "Ctrl+Alt+B"
    );
    assert_eq!(shortcut_label(&keys_off_mac("quit"), false), "Ctrl+Q");
}

#[test]
fn keys_for_mac_keeps_the_table_spelling() {
    let redo = command("redo").expect("redo");
    assert_eq!(redo.keys_for(true).as_deref(), Some("cmd-shift-z"));
    assert_eq!(redo.keys_for(false).as_deref(), Some("ctrl-y"));
    let theme = command("toggle-theme").expect("toggle-theme");
    assert_eq!((theme.keys_for(true), theme.keys_for(false)), (None, None));
    assert_eq!(
        command("undo").map(|c| c.label_id()).as_deref(),
        Some("cmd-undo")
    );
    assert_eq!(
        command("undo").map(|c| c.tooltip_id()).as_deref(),
        Some("cmd-undo-tip")
    );
}
