//! UI-12.2: the modal overlays and confirmation questions in `app.slint`
//! are listed here with a reason. A new dialog fails the build until it is
//! added, so "ask first" stays the exception: undoable changes happen at once
//! with an undo toast (UI-10.8), and a confirmation is only for what Undo
//! cannot bring back (UI-12.1, `docs/engineering-standards.md` section 11).

use std::collections::BTreeSet;
use std::path::Path;

/// Modal overlays (`if root.<name>: Rectangle`) and why each may exist.
const OVERLAYS: &[(&str, &str)] = &[
    (
        "confirm-open",
        "the one confirmation dialog; its kinds are listed in CONFIRMATIONS",
    ),
    (
        "prompt-open",
        "asks for text the app cannot guess (a name); it is input, not a confirmation",
    ),
    (
        "outline-open",
        "editor for a whole attack tree as an outline; input",
    ),
    ("password-open", "UI-12: change password"),
    (
        "palette-open",
        "command palette: pick a command by typing; not a confirmation",
    ),
];

/// Confirmation kinds (`confirm("kind", ...)`) and why Undo cannot cover them.
const CONFIRMATIONS: &[(&str, &str)] = &[
    (
        "delete-project",
        "UI-12: deletes the project and its history on the server",
    ),
    (
        "break-lock",
        "UI-12: discards another user's unsaved changes",
    ),
    (
        "revoke-session",
        "UI-12: ends a session, which is another user's access",
    ),
    (
        "delete-user",
        "an account and its sessions go; not in the project history",
    ),
    (
        "remove-member",
        "takes a person's access to a project; not in the project history",
    ),
    (
        "password-login-off",
        "logs everyone out of password sessions; can lock people out",
    ),
    (
        "log-out",
        "closes the project and the connection; kept until the owner decides to drop it",
    ),
];

fn source() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/app.slint");
    std::fs::read_to_string(path).expect("app.slint")
}

/// Every `<open>` of `if root.<open>: Rectangle {` and every `kind` of
/// `confirm("kind"` / `confirm-named("kind"`.
fn found(text: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut overlays = BTreeSet::new();
    let mut kinds = BTreeSet::new();
    for line in text.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("if root.")
            && let Some((name, tail)) = rest.split_once(':')
            && name.ends_with("-open")
            && tail.trim_start().starts_with("Rectangle")
        {
            overlays.insert(name.to_owned());
        }
    }
    for marker in ["root.confirm(\"", "root.confirm-named(\""] {
        for part in text.split(marker).skip(1) {
            if let Some((kind, _)) = part.split_once('"') {
                kinds.insert(kind.to_owned());
            }
        }
    }
    (overlays, kinds)
}

fn names(list: &[(&str, &str)]) -> BTreeSet<String> {
    list.iter().map(|(n, _)| (*n).to_owned()).collect()
}

#[test]
fn every_modal_overlay_is_on_the_allow_list() {
    let (overlays, _) = found(&source());
    assert_eq!(
        overlays,
        names(OVERLAYS),
        "a dialog was added or removed: update OVERLAYS with a reason (UI-12)"
    );
}

#[test]
fn only_irreversible_actions_ask_for_confirmation() {
    let (_, kinds) = found(&source());
    let allowed = names(CONFIRMATIONS);
    let extra: Vec<_> = kinds.difference(&allowed).collect();
    assert!(
        extra.is_empty(),
        "{extra:?} asks for confirmation: make it undoable with an undo toast, or add it to CONFIRMATIONS with a reason (UI-12)"
    );
}
