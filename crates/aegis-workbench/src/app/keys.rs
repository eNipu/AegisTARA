//! The workbench's GPUI actions and their key bindings, built from the
//! command table so a shortcut is written in one place only.

use std::rc::Rc;

use aegis_client::command_table::commands;
use gpui::{Action, App, KeyBinding, KeyBindingContextPredicate, actions};

actions!(
    workbench,
    [
        ExplorerDown,
        ExplorerUp,
        ExplorerExpand,
        ExplorerCollapse,
        OpenSelected,
        PickUp,
        DropHere,
        Undo,
        Redo
    ]
);

/// The action of a command id, for the commands the workbench handles.
fn action(id: &str) -> Option<Box<dyn Action>> {
    Some(match id {
        "explorer-down" => Box::new(ExplorerDown),
        "explorer-up" => Box::new(ExplorerUp),
        "explorer-expand" => Box::new(ExplorerExpand),
        "explorer-collapse" => Box::new(ExplorerCollapse),
        "open-selected" => Box::new(OpenSelected),
        "pick-up" => Box::new(PickUp),
        "drop-here" => Box::new(DropHere),
        "undo" => Box::new(Undo),
        "redo" => Box::new(Redo),
        _ => return None,
    })
}

/// Binds the keys of every handled command, on this platform's spelling.
pub fn bind_keys(cx: &mut App) {
    let mac = cfg!(target_os = "macos");
    let mut bindings = Vec::new();
    for spec in commands() {
        let (Some(action), Some(keys)) = (action(spec.id), spec.keys_for(mac)) else {
            continue;
        };
        let context = spec
            .context
            .and_then(|c| KeyBindingContextPredicate::parse(c).ok())
            .map(Rc::new);
        match KeyBinding::load(&keys, action, context, false, None, cx) {
            Ok(binding) => bindings.push(binding),
            Err(err) => eprintln!("cannot bind {keys} to {}: {err}", spec.id),
        }
    }
    cx.bind_keys(bindings);
}
