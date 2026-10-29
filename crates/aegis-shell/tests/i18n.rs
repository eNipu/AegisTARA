//! UI translation (jobs 066 to 068): every `@tr(...)` key in the `.slint`
//! files has an entry in each bundled locale (L10N-03), and the bundled
//! catalogs select a language at run time.

use std::collections::BTreeSet;
use std::path::Path;

/// The `@tr("...")` keys in the Slint sources, in order.
fn slint_keys() -> Vec<String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut keys = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("ui dir") {
        let entry = entry.expect("entry");
        if entry.path().extension().and_then(|e| e.to_str()) != Some("slint") {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).expect("slint source");
        let mut rest = text.as_str();
        while let Some(at) = rest.find("@tr(") {
            rest = &rest[at + 4..];
            let Some(quote) = rest.find('"') else { break };
            let inner = &rest[quote + 1..];
            let Some(end) = inner.find('"') else { break };
            keys.push(inner[..end].to_owned());
            rest = &inner[end..];
        }
    }
    keys
}

/// The msgids of a `.po` file.
fn po_msgids(path: &Path) -> BTreeSet<String> {
    let text = std::fs::read_to_string(path).expect("po file");
    let mut ids = BTreeSet::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        if let Some(rest) = line.strip_prefix("msgid \"") {
            let mut value = rest.strip_suffix('"').unwrap_or(rest).to_owned();
            // Fold continuation lines.
            while let Some(next) = lines.next()
                && let Some(cont) = next.strip_prefix('"')
                && let Some(inner) = cont.strip_suffix('"')
            {
                value.push_str(inner);
            }
            ids.insert(value);
        }
    }
    ids
}

#[test]
fn every_shipped_locale_has_every_key() {
    let keys = slint_keys();
    assert!(!keys.is_empty(), "the Slint sources use @tr");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("translations");
    for locale in ["ja", "pseudo"] {
        let po = dir.join(locale).join("LC_MESSAGES").join("aegis-shell.po");
        let ids = po_msgids(&po);
        for key in &keys {
            assert!(ids.contains(key), "{locale} lacks the key {key:?}");
        }
    }
}
