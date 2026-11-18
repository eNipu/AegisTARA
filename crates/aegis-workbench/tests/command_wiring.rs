//! Every command of the table has an icon and a handler, and every command
//! with keys gets a binding (WBX-11).
#![cfg(any(target_os = "macos", target_os = "windows"))]

use aegis_client::command_table::commands;
use aegis_workbench::app::{bindings, dispatch_id, icon};
use gpui::TestAppContext;

#[test]
fn every_command_has_an_icon_and_a_handler() {
    for spec in commands() {
        assert!(
            icon(spec.icon).is_some(),
            "{}: no icon {}",
            spec.id,
            spec.icon
        );
        assert!(dispatch_id(spec.id).is_some(), "{}: no handler", spec.id);
    }
    assert!(icon("no-such-icon").is_none());
    assert!(dispatch_id("no-such-command").is_none());
}

#[gpui::test]
fn every_command_with_keys_gets_one_binding(cx: &mut TestAppContext) {
    let mac = cfg!(target_os = "macos");
    let with_keys = commands()
        .iter()
        .filter(|c| c.keys_for(mac).is_some())
        .count();
    let bound = cx.update(|cx| bindings(cx).len());
    assert_eq!(bound, with_keys);
}
