//! Description text on every element and notes on the project (job 064,
//! FND-01): plain long text with caps, set by one op, undone with its
//! inverse, kept in snapshots and op logs, and compared by diff.

use std::sync::Arc;

use aegis_model::{
    AssetId, ElementRef, EntityKind, MAX_DESCRIPTION_CHARS, MAX_NOTES_CHARS, Methodology,
    ModelError, Op, OpCodec, OpLogReader, OpLogWriter, Project, Provenance,
};

fn methodology() -> Arc<Methodology> {
    Arc::new(Methodology::iso21434_r155().expect("builtin"))
}

fn with_asset() -> (Project, AssetId) {
    let m = methodology();
    let integrity = m.property("integrity").expect("property");
    let mut project = Project::new(Arc::clone(&m));
    let id = AssetId::new_v7();
    project
        .apply(Op::AddAsset {
            id,
            name: "Brake ECU".into(),
            properties: [integrity].into_iter().collect(),
            provenance: Provenance::default(),
        })
        .expect("asset");
    (project, id)
}

fn set(target: ElementRef, text: &str, who: &str, at: u64) -> Op {
    Op::SetDescription {
        target,
        text: text.into(),
        provenance: Provenance::human(who, at),
    }
}

#[test]
fn a_description_is_set_read_and_undone_with_its_provenance() {
    let (mut project, id) = with_asset();
    let target = ElementRef::Asset(id);
    assert_eq!(project.description(target), None);
    let before = project.clone();

    let first = project
        .apply(set(
            target,
            "Controls the brake pressure.\nSafety relevant.",
            "alice",
            5,
        ))
        .expect("set");
    assert_eq!(
        project.description(target),
        Some("Controls the brake pressure.\nSafety relevant.")
    );
    let who = project.description_provenance(target).expect("provenance");
    assert_eq!((who.actor(), who.timestamp_ms()), ("alice", 5));

    let second = project
        .apply(set(target, "Revised.", "bob", 9))
        .expect("set");
    assert_eq!(project.description(target), Some("Revised."));
    // The inverse of the second change restores the first text and author.
    project.apply(second).expect("undo");
    assert_eq!(
        project
            .description_provenance(target)
            .map(|p| p.actor().to_owned()),
        Some("alice".to_owned())
    );
    project.apply(first).expect("undo");
    assert_eq!(project.description(target), None);
    assert_eq!(project, before, "undo restores the whole project");
}

#[test]
fn an_empty_text_clears_the_description_and_the_notes_work_the_same_way() {
    let (mut project, id) = with_asset();
    let target = ElementRef::Asset(id);
    project.apply(set(target, "x", "a", 1)).expect("set");
    project.apply(set(target, "", "a", 2)).expect("clear");
    assert_eq!(project.description(target), None);
    assert_eq!(project.description_count(), 0);

    project
        .apply(set(ElementRef::Project, "Scope: ECU only.", "a", 3))
        .expect("notes");
    assert_eq!(
        project.description(ElementRef::Project),
        Some("Scope: ECU only.")
    );
    assert_eq!(project.description_count(), 1);
}

#[test]
fn text_is_plain_and_capped() {
    let (mut project, id) = with_asset();
    let target = ElementRef::Asset(id);
    let ok = "é".repeat(MAX_DESCRIPTION_CHARS);
    project.apply(set(target, &ok, "a", 1)).expect("at the cap");
    let long = "é".repeat(MAX_DESCRIPTION_CHARS + 1);
    assert_eq!(
        project.validate(&set(target, &long, "a", 2)),
        Err(ModelError::DescriptionTooLong {
            max: MAX_DESCRIPTION_CHARS
        })
    );
    // Notes may be longer than an element's description.
    let notes = "n".repeat(MAX_NOTES_CHARS);
    project
        .apply(set(ElementRef::Project, &notes, "a", 3))
        .expect("notes at the cap");
    assert_eq!(
        project.validate(&set(ElementRef::Project, &format!("{notes}n"), "a", 4)),
        Err(ModelError::DescriptionTooLong {
            max: MAX_NOTES_CHARS
        })
    );
    // Tabs and line breaks are text; other control characters and NUL are not.
    assert_eq!(project.validate(&set(target, "a\tb\nc", "a", 5)), Ok(()));
    for bad in ["a\u{0}b", "a\rb", "a\u{7}b", "a\u{1b}[31m"] {
        assert_eq!(
            project.validate(&set(target, bad, "a", 6)),
            Err(ModelError::InvalidText),
            "{bad:?}"
        );
    }
}

#[test]
fn only_live_elements_can_be_described() {
    let (project, _) = with_asset();
    let ghost = ElementRef::Asset(AssetId::new_v7());
    assert!(matches!(
        project.validate(&set(ghost, "x", "a", 1)),
        Err(ModelError::UnknownAsset(_))
    ));
}

#[test]
fn an_element_with_a_description_cannot_be_removed_until_it_is_cleared() {
    let (mut project, id) = with_asset();
    let target = ElementRef::Asset(id);
    project
        .apply(set(target, "Keep this text.", "alice", 1))
        .expect("set");
    assert_eq!(
        project.validate(&Op::RemoveAsset { id }),
        Err(ModelError::DescriptionNotCleared {
            kind: EntityKind::Asset
        })
    );

    // A delete is "clear, then remove": its inverses, applied in reverse
    // order, bring back the element and then its text and author.
    let before = project.clone();
    let undo_clear = project.apply(set(target, "", "alice", 2)).expect("clear");
    let undo_remove = project.apply(Op::RemoveAsset { id }).expect("remove");
    assert_eq!(project.asset_count(), 0);
    project.apply(undo_remove).expect("re-add");
    project.apply(undo_clear).expect("restore text");
    assert_eq!(project, before);
}

#[test]
fn descriptions_survive_save_load_and_leave_old_files_unchanged() {
    let (mut project, id) = with_asset();
    let mut plain = Vec::new();
    project.save(&mut plain).expect("save");
    assert!(
        !String::from_utf8_lossy(&plain).contains("description"),
        "a project without descriptions is written as before"
    );

    project
        .apply(set(ElementRef::Asset(id), "Line one\nLine two", "alice", 7))
        .expect("set");
    project
        .apply(set(ElementRef::Project, "Notes", "bob", 8))
        .expect("notes");
    let mut bytes = Vec::new();
    project.save(&mut bytes).expect("save");
    let loaded = Project::load(bytes.as_slice(), methodology()).expect("load");
    assert_eq!(loaded, project);
    assert_eq!(loaded.description(ElementRef::Project), Some("Notes"));
    let mut again = Vec::new();
    loaded.save(&mut again).expect("save");
    assert_eq!(again, bytes, "saving is deterministic");
}

#[test]
fn a_snapshot_over_the_total_text_cap_is_refused() {
    let (mut project, id) = with_asset();
    let m = methodology();
    // Many elements, each below its own cap, add up past the project cap.
    let integrity = m.property("integrity").expect("property");
    let per = "x".repeat(MAX_DESCRIPTION_CHARS);
    let mut added = 0usize;
    let mut last = Ok(());
    for _ in 0..4_000 {
        let aid = AssetId::new_v7();
        project
            .apply(Op::AddAsset {
                id: aid,
                name: "A".into(),
                properties: [integrity].into_iter().collect(),
                provenance: Provenance::default(),
            })
            .expect("asset");
        let op = set(ElementRef::Asset(aid), &per, "a", 1);
        last = project.validate(&op);
        if last.is_err() {
            break;
        }
        project.apply(op).expect("set");
        added += 1;
    }
    assert!(
        matches!(last, Err(ModelError::DescriptionTotalTooLarge { .. })),
        "{last:?}"
    );
    assert!(
        added >= 1_000,
        "the cap is generous: {added} full descriptions fit"
    );
    let _ = id;
}

#[test]
fn the_op_log_carries_descriptions_and_refuses_oversized_text() {
    let (project, id) = with_asset();
    let m = methodology();
    let codec = OpCodec::new(Arc::clone(&m));
    let op = set(ElementRef::Asset(id), "Described in the log.", "alice", 11);

    let mut log = Vec::new();
    let mut writer = OpLogWriter::create(&mut log, Arc::clone(&m)).expect("writer");
    writer.append(&op).expect("append");
    drop(writer);
    let read: Vec<_> = OpLogReader::open(log.as_slice(), Arc::clone(&m))
        .expect("header")
        .map(|r| r.expect("record").op)
        .collect();
    assert_eq!(read, vec![op.clone()]);

    let bytes = codec.encode(&op).expect("encodes");
    assert_eq!(codec.decode(&bytes).expect("decodes"), op);
    let _ = project;

    // Oversized text is refused while decoding, before any validation (R3).
    let huge = Op::SetDescription {
        target: ElementRef::Project,
        text: "x".repeat(MAX_NOTES_CHARS * 8).into(),
        provenance: Provenance::default(),
    };
    assert!(
        codec.encode(&huge).is_err() || codec.decode(&codec.encode(&huge).expect("e")).is_err()
    );
}

#[test]
fn diff_reports_changed_descriptions_and_notes() {
    let (base, id) = with_asset();
    let mut current = base.clone();
    current
        .apply(set(ElementRef::Asset(id), "New text", "a", 1))
        .expect("set");
    current
        .apply(set(ElementRef::Project, "Notes", "a", 2))
        .expect("notes");
    let diff = base.diff(&current).expect("diff");
    assert!(diff.notes_changed());
    assert_eq!(diff.assets().modified(), [id]);
    assert!(diff.risk_deltas().is_empty(), "text changes no number");
    assert!(!base.diff(&base).expect("diff").notes_changed());
    assert!(
        base.diff(&base)
            .expect("diff")
            .assets()
            .modified()
            .is_empty()
    );
}

fn set_languages(langs: &[&str], who: &str, at: u64) -> Op {
    Op::SetProjectLanguages {
        langs: langs.iter().map(|l| Arc::from(*l)).collect(),
        provenance: Provenance::human(who, at),
    }
}

#[test]
fn project_languages_are_set_checked_undone_and_compared_by_diff() {
    let (mut project, id) = with_asset();
    assert!(project.languages().is_empty(), "a new project has none");
    let base = project.clone();

    let undo = project
        .apply(set_languages(&["ja", "de"], "alice", 3))
        .expect("set");
    assert_eq!(project.languages(), [Arc::from("ja"), Arc::from("de")]);
    assert!(base.diff(&project).expect("diff").languages_changed());
    assert!(!base.diff(&base).expect("diff").languages_changed());
    assert_eq!(
        undo,
        Op::SetProjectLanguages {
            langs: Arc::from([]),
            provenance: Provenance::default(),
        }
    );
    project.apply(undo).expect("undo");
    assert_eq!(project, base);

    for (langs, err) in [
        (&["ja", "ja"][..], ModelError::DuplicateLanguage),
        (&[""][..], ModelError::InvalidText),
        (&["ja\n"][..], ModelError::InvalidText),
        (
            &["a", "b", "c", "d", "e", "f", "g", "h", "i"][..],
            ModelError::TooManyLanguages { max: 8 },
        ),
    ] {
        assert_eq!(project.validate(&set_languages(langs, "a", 1)), Err(err));
    }

    // Translations in a language the project no longer lists stay.
    project
        .apply(set(ElementRef::Asset(id), "Text", "a", 4))
        .expect("set");
    project
        .apply(Op::SetDescriptionTranslation {
            target: ElementRef::Asset(id),
            lang: "fr".into(),
            text: "Texte".into(),
            provenance: Provenance::default(),
        })
        .expect("translate");
    project.apply(set_languages(&["ja"], "a", 5)).expect("set");
    assert_eq!(
        project.description_translation(ElementRef::Asset(id), "fr"),
        Some("Texte")
    );
}

#[test]
fn project_languages_survive_save_load_and_the_op_log() {
    let (mut project, _) = with_asset();
    let m = methodology();
    let mut plain = Vec::new();
    project.save(&mut plain).expect("save");
    assert!(!String::from_utf8_lossy(&plain).contains("languages"));

    let op = set_languages(&["ja"], "alice", 9);
    project.apply(op.clone()).expect("set");
    let mut bytes = Vec::new();
    project.save(&mut bytes).expect("save");
    let loaded = Project::load(bytes.as_slice(), Arc::clone(&m)).expect("load");
    assert_eq!(loaded.languages(), [Arc::from("ja")]);
    assert_eq!(loaded, project);

    let codec = OpCodec::new(m);
    assert_eq!(
        codec
            .decode(&codec.encode(&op).expect("encode"))
            .expect("decode"),
        op
    );
}
