use aegis_client::view::{
    ASSET_COLUMNS, CONTROL_COLUMNS, ColumnKind, DAMAGE_COLUMNS, GOAL_COLUMNS, Page, PageLayout,
    THREAT_COLUMNS,
};

#[test]
fn table_pages_have_id_then_name() {
    for page in [
        Page::Assets,
        Page::Damage,
        Page::Threats,
        Page::Risks,
        Page::Controls,
        Page::Goals,
    ] {
        match page.layout() {
            PageLayout::Table(cols) => {
                assert!(cols.len() >= 2);
                assert_eq!(cols[0].kind, ColumnKind::Id);
                assert_eq!(cols[1].kind, ColumnKind::Name);
                for c in cols {
                    assert!(!c.label.is_empty());
                    assert!(c.width > 0);
                }
            }
            other => panic!("{page:?} is not a table: {other:?}"),
        }
    }
}

#[test]
fn constants_match_page_layouts() {
    assert_eq!(
        ASSET_COLUMNS,
        match Page::Assets.layout() {
            PageLayout::Table(s) => s,
            _ => panic!(),
        }
    );
    assert_eq!(
        DAMAGE_COLUMNS,
        match Page::Damage.layout() {
            PageLayout::Table(s) => s,
            _ => panic!(),
        }
    );
    assert_eq!(
        THREAT_COLUMNS,
        match Page::Threats.layout() {
            PageLayout::Table(s) => s,
            _ => panic!(),
        }
    );
    assert_eq!(
        CONTROL_COLUMNS,
        match Page::Controls.layout() {
            PageLayout::Table(s) => s,
            _ => panic!(),
        }
    );
    assert_eq!(
        GOAL_COLUMNS,
        match Page::Goals.layout() {
            PageLayout::Table(s) => s,
            _ => panic!(),
        }
    );
}

#[test]
fn exact_kind_sequences() {
    let kinds = |cols: &[aegis_client::view::ColumnSpec]| -> Vec<ColumnKind> {
        cols.iter().map(|c| c.kind).collect()
    };
    assert_eq!(
        kinds(ASSET_COLUMNS),
        vec![
            ColumnKind::Id,
            ColumnKind::Name,
            ColumnKind::Links("asset-component"),
            ColumnKind::Computed,
            ColumnKind::Computed,
        ]
    );
    assert_eq!(
        kinds(DAMAGE_COLUMNS),
        vec![
            ColumnKind::Id,
            ColumnKind::Name,
            ColumnKind::Rating,
            ColumnKind::Computed,
        ]
    );
    assert_eq!(
        kinds(THREAT_COLUMNS),
        vec![
            ColumnKind::Id,
            ColumnKind::Name,
            ColumnKind::Links("threat-asset"),
            ColumnKind::Links("threat-damage"),
            ColumnKind::Computed,
            ColumnKind::Computed,
            ColumnKind::Rating,
        ]
    );
    assert_eq!(
        kinds(CONTROL_COLUMNS),
        vec![
            ColumnKind::Id,
            ColumnKind::Name,
            ColumnKind::Computed,
            ColumnKind::Computed,
            ColumnKind::Computed,
        ]
    );
    assert_eq!(
        kinds(GOAL_COLUMNS),
        vec![
            ColumnKind::Id,
            ColumnKind::Name,
            ColumnKind::Links("goal-controls"),
            ColumnKind::Computed,
        ]
    );
}

#[test]
fn computed_and_id_not_editable() {
    assert!(!ColumnKind::Computed.editable());
    assert!(!ColumnKind::Id.editable());
    for cols in [
        ASSET_COLUMNS,
        DAMAGE_COLUMNS,
        THREAT_COLUMNS,
        CONTROL_COLUMNS,
        GOAL_COLUMNS,
    ] {
        assert!(cols.iter().any(|c| c.kind == ColumnKind::Computed));
    }
}

#[test]
fn every_page_has_a_layout() {
    for page in Page::ALL {
        let _ = page.layout();
    }
}
