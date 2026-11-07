//! Item definition text (job 132, ARC-05): the item's description and scope
//! are description targets with translations; exact inverses, the store
//! round trip, the diff flag, and projects without them load unchanged.

use std::sync::Arc;

use aegis_model::{ElementRef, MAX_NOTES_CHARS, Methodology, Op, Project, Provenance};

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

fn text(target: ElementRef, text: &str) -> Op {
    Op::SetDescription {
        target,
        text: text.into(),
        provenance: Provenance::default(),
    }
}

#[test]
fn description_and_scope_are_set_translated_undone_and_saved() {
    let m = methodology();
    let mut p = Project::new(Arc::clone(&m));
    let empty = p.clone();
    let mut out = Vec::new();
    p.save(&mut out).expect("save");
    assert!(!String::from_utf8_lossy(&out).contains("item_"));

    let u1 = p
        .apply(text(
            ElementRef::ItemDescription,
            "A brake-by-wire ECU with a CAN and an Ethernet port.",
        ))
        .expect("description");
    let u2 = p
        .apply(text(
            ElementRef::ItemScope,
            "In scope: the ECU and its firmware. Out of scope: the vehicle network.",
        ))
        .expect("scope");
    p.apply(Op::SetProjectLanguages {
        langs: vec![Arc::from("ja")].into(),
        provenance: Provenance::default(),
    })
    .expect("languages");
    p.apply(Op::SetDescriptionTranslation {
        target: ElementRef::ItemScope,
        lang: "ja".into(),
        text: "対象: ECU とファームウェア。対象外: 車両ネットワーク。".into(),
        provenance: Provenance::default(),
    })
    .expect("translation");
    assert_eq!(
        p.description_translation(ElementRef::ItemScope, "ja"),
        Some("対象: ECU とファームウェア。対象外: 車両ネットワーク。")
    );
    assert!(p.visual_id(ElementRef::ItemDescription).is_none());
    assert_eq!(ElementRef::ItemScope.max_chars(), MAX_NOTES_CHARS);

    let mut out = Vec::new();
    p.save(&mut out).expect("save");
    let loaded = Project::load(out.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded, p);

    let diff = empty.diff(&p).expect("diff");
    assert!(diff.item_text_changed() && !diff.notes_changed());

    // Clear the translation, then undo both texts back to the empty project.
    p.apply(Op::SetDescriptionTranslation {
        target: ElementRef::ItemScope,
        lang: "ja".into(),
        text: "".into(),
        provenance: Provenance::default(),
    })
    .expect("clear translation");
    p.apply(Op::SetProjectLanguages {
        langs: Vec::<Arc<str>>::new().into(),
        provenance: Provenance::default(),
    })
    .expect("no languages");
    p.apply(u2).expect("undo scope");
    p.apply(u1).expect("undo description");
    assert_eq!(p, empty);
}
