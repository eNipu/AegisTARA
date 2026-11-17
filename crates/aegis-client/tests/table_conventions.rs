//! The one table convention (UI-14): every table's columns come from its
//! `ColumnSpec` list and its cells from `table_rows`.

use std::collections::HashSet;
use std::sync::Arc;

use aegis_client::commands::Edit;
use aegis_client::i18n::{Catalog, Language};
use aegis_client::pages::{self, Vocabulary};
use aegis_client::relations::RelationKey;
use aegis_client::sample::sample_ops;
use aegis_client::table::table_rows;
use aegis_client::view::{
    ColumnKind, ColumnLabel, Page, PageLayout, RiskView, TableId, threat_rows,
};
use aegis_client::workspace::{Origin, Workspace};
use aegis_model::{ClaimId, GoalId, Methodology};

/// The sample project plus a goal and a claim, so every table has a row.
fn workspace_of(m: Methodology) -> Workspace {
    let m = Arc::new(m);
    let mut ws = Workspace::empty(Arc::clone(&m));
    ws.apply(sample_ops(&m, "demo", 1).expect("sample"), Origin::Edit)
        .expect("applies");
    let edits = [
        Edit::AddGoal {
            id: GoalId::new_v7(),
            name: "Only authenticated testers open sessions".into(),
        },
        Edit::AddClaim {
            id: ClaimId::new_v7(),
            name: "Firmware is signed by the supplier".into(),
        },
    ];
    for edit in &edits {
        let ops = edit.to_ops(ws.project(), "demo", 2).expect("edit");
        ws.apply(ops, Origin::Edit).expect("applies");
    }
    ws
}

fn workspace() -> Workspace {
    workspace_of(Methodology::iso21434_r155().expect("builtin"))
}

fn english(ws: &Workspace) -> (Vocabulary, Catalog) {
    (
        pages::vocabulary(ws.project().methodology()),
        Catalog::load(Language::En),
    )
}

/// The texts of the columns shown by default, row by row.
fn shown(ws: &Workspace, table: TableId) -> Vec<Vec<String>> {
    let (vocab, catalog) = english(ws);
    table_rows(ws, table, &vocab, &catalog)
        .into_iter()
        .map(|row| {
            table
                .columns()
                .iter()
                .zip(row.cells)
                .filter(|(c, _)| c.visible)
                .map(|(_, cell)| cell.text)
                .collect()
        })
        .collect()
}

fn label(r: Option<&RiskView>, unrated: &str) -> String {
    r.map_or_else(|| unrated.to_owned(), |r| r.label.clone())
}

/// No visible loss: each table shows, by default, the columns its page
/// shows today, in the same order, built here the way the page builds them.
#[test]
fn every_table_shows_what_its_page_shows_today() {
    let ws = workspace();
    let (vocab, _) = english(&ws);
    let n = |n: usize| n.to_string();
    let filled = |rows: Vec<Vec<String>>| {
        assert!(!rows.is_empty(), "the fixture fills every table");
        rows
    };

    let assets = pages::asset_rows(&ws)
        .into_iter()
        .map(|a| {
            let props: Vec<&str> = a
                .properties
                .iter()
                .zip(&vocab.properties)
                .filter(|(on, _)| **on)
                .map(|(_, l)| l.as_str())
                .collect();
            vec![
                a.visual_id,
                a.name,
                props.join(", "),
                n(a.threats),
                label(a.worst.as_ref(), ""),
            ]
        })
        .collect();
    assert_eq!(shown(&ws, TableId::Assets), filled(assets));

    let damage = pages::damage_rows(&ws)
        .into_iter()
        .map(|d| vec![d.visual_id, d.name, d.impacts_text, n(d.threats)])
        .collect();
    assert_eq!(shown(&ws, TableId::Damage), filled(damage));

    let threats = threat_rows(&ws)
        .into_iter()
        .map(|t| {
            let scales = match t.scales {
                Some(true) => "yes",
                Some(false) => "no",
                None => "",
            };
            vec![
                t.visual_id,
                t.name,
                t.asset,
                scales.to_owned(),
                label(t.inherent.as_ref(), "not rated"),
                label(t.residual.as_ref(), "not rated"),
            ]
        })
        .collect();
    assert_eq!(shown(&ws, TableId::Threats), filled(threats));

    let risks = pages::risk_rows(&ws)
        .into_iter()
        .map(|r| {
            let before = label(r.inherent.as_ref(), "not rated");
            let after = label(r.residual.as_ref(), "not rated");
            vec![
                r.visual_id,
                r.threat,
                r.damage,
                r.impact,
                r.feasibility,
                r.rationale,
                before,
                after,
                r.treatment,
            ]
        })
        .collect();
    assert_eq!(shown(&ws, TableId::Risks), filled(risks));

    let controls = pages::control_rows(&ws)
        .into_iter()
        .map(|c| vec![c.visual_id, c.name, n(c.steps), n(c.goals)])
        .collect();
    assert_eq!(shown(&ws, TableId::Controls), filled(controls));

    let goals = pages::goal_rows(&ws)
        .into_iter()
        .map(|g| {
            vec![
                g.visual_id,
                g.name,
                g.responsible.join(", "),
                n(g.controls),
                n(g.threats),
            ]
        })
        .collect();
    assert_eq!(shown(&ws, TableId::Goals), filled(goals));

    let claims = pages::claim_rows(&ws)
        .into_iter()
        .map(|c| vec![c.visual_id, c.name, c.responsible.join(", "), n(c.threats)])
        .collect();
    assert_eq!(shown(&ws, TableId::Claims), filled(claims));

    let problems = pages::problems(&ws)
        .into_iter()
        .map(|p| {
            let severity = if p.error { "Blocking" } else { "Warning" };
            vec![p.visual_id, severity.to_owned(), p.text]
        })
        .collect();
    assert_eq!(shown(&ws, TableId::Problems), filled(problems));
}

/// Hidden columns have cells too, every column key is known to the cell
/// lookup (an unknown one shows as `[key]`), and a row opens its element.
#[test]
fn every_column_has_a_cell_in_every_row() {
    let ws = workspace();
    let (vocab, catalog) = english(&ws);
    for table in TableId::ALL {
        let columns = table.columns();
        for row in table_rows(&ws, table, &vocab, &catalog) {
            assert_eq!(row.cells.len(), columns.len(), "{table:?}");
            for (c, cell) in columns.iter().zip(&row.cells) {
                assert_ne!(cell.text, format!("[{}]", c.key), "{table:?}");
            }
            assert!(row.target.is_some() || table == TableId::Problems);
        }
    }
    // A hidden column carries its value: the risks a control lowers.
    let lowers = pages::control_rows(&ws).remove(0).lowers;
    let row = table_rows(&ws, TableId::Controls, &vocab, &catalog).remove(0);
    let column = TableId::Controls.columns()[4];
    assert_eq!((column.key, column.visible), ("lowers", false));
    assert_eq!(row.cells[4].text, lowers.to_string());
}

#[test]
fn a_risk_cell_carries_its_color_bucket() {
    let ws = workspace();
    let (vocab, catalog) = english(&ws);
    let rated = threat_rows(&ws).remove(0).inherent.expect("rated");
    let row = table_rows(&ws, TableId::Threats, &vocab, &catalog).remove(0);
    assert_eq!(row.cells[4].tone, Some(rated.tone));
    assert_eq!(row.cells[1].tone, None);
}

#[test]
fn columns_have_unique_keys_and_start_with_the_id() {
    for table in TableId::ALL {
        let columns = table.columns();
        let keys: HashSet<&str> = columns.iter().map(|c| c.key).collect();
        assert_eq!(keys.len(), columns.len(), "{table:?}: a key repeats");
        assert_eq!(columns[0].kind, ColumnKind::Id, "{table:?}");
        assert!(columns[0].visible, "{table:?}");
        if table != TableId::Problems {
            assert_eq!(columns[1].kind, ColumnKind::Name, "{table:?}");
        }
        for c in columns {
            assert!(c.width > 0, "{table:?} {}", c.key);
            if let ColumnKind::Links(relation) = c.kind {
                assert!(!relation.as_str().is_empty(), "{table:?} {}", c.key);
            }
        }
    }
}

/// Every heading is a real catalog message in every language, never `[id]`.
#[test]
fn headings_resolve_in_every_language() {
    let ws = workspace();
    for language in [Language::En, Language::Ja, Language::Pseudo] {
        let catalog = Catalog::load(language);
        let vocab = pages::vocabulary_for_lang(ws.project().methodology(), language.code());
        for table in TableId::ALL {
            for c in table.columns() {
                let heading = c.heading(&vocab.element_names, &catalog);
                assert!(
                    !heading.is_empty() && !heading.starts_with('['),
                    "{table:?} {} in {language}: {heading:?}",
                    c.key
                );
            }
        }
    }
}

/// A heading that names an element type follows the methodology pack.
#[test]
fn element_headings_follow_the_pack() {
    let catalog = Catalog::load(Language::En);
    let heading = |m: Methodology, table: TableId| {
        let names = pages::vocabulary(&m).element_names;
        let c = table.columns()[1];
        assert!(matches!(c.label, ColumnLabel::Element(_)));
        (c.heading(&names, &catalog), names)
    };
    let (iso, names) = heading(Methodology::iso21434_r155().expect("iso"), TableId::Damage);
    assert_eq!(iso, &*names.damage_scenario.singular);
    let (cra, names) = heading(Methodology::eu_cra().expect("cra"), TableId::Damage);
    assert_eq!(cra, &*names.damage_scenario.singular);
    assert_ne!(iso, cra, "the packs name the element differently");
    let (goal, names) = heading(Methodology::eu_cra().expect("cra"), TableId::Goals);
    assert_eq!(goal, &*names.cybersecurity_goal.singular);
}

/// Exactly these cells may be edited in place; worked-out values and
/// every problem cell may not.
#[test]
fn only_inputs_are_editable() {
    let editable = |table: TableId| -> Vec<&str> {
        table
            .columns()
            .iter()
            .filter(|c| c.kind.editable())
            .map(|c| c.key)
            .collect()
    };
    assert_eq!(
        editable(TableId::Assets),
        ["name", "component", "properties"]
    );
    assert_eq!(editable(TableId::Damage), ["name", "impact"]);
    assert_eq!(editable(TableId::Threats), ["name", "asset", "scales"]);
    assert_eq!(editable(TableId::Risks), ["name", "asset", "treatment"]);
    assert_eq!(editable(TableId::Controls), ["name"]);
    assert_eq!(editable(TableId::Goals), ["name", "responsible"]);
    assert_eq!(editable(TableId::Claims), ["name", "responsible"]);
    assert_eq!(editable(TableId::Problems), [""; 0]);
}

#[test]
fn every_table_belongs_to_one_page_and_layouts_agree() {
    let mut seen = Vec::new();
    for page in Page::ALL {
        let tables = page.tables();
        seen.extend_from_slice(tables);
        match (page.layout(), tables.first()) {
            (PageLayout::Table(columns), Some(first)) => assert_eq!(columns, first.columns()),
            (PageLayout::Canvas, None) => assert!(matches!(page, Page::Item | Page::Trees)),
            (PageLayout::Custom, None) => {}
            (layout, _) => panic!("{page:?}: {layout:?} with {tables:?}"),
        }
    }
    assert_eq!(seen.len(), TableId::ALL.len(), "a table is on two pages");
    assert_eq!(
        seen.into_iter().collect::<HashSet<_>>(),
        HashSet::from(TableId::ALL)
    );
    assert_eq!(Page::Goals.tables(), [TableId::Goals, TableId::Claims]);
}

/// The Responsible column sits after the name on both tables and points at
/// the two registry rows added for party links.
#[test]
fn responsible_column_is_a_links_column_on_both_tables() {
    for (table, key) in [
        (TableId::Goals, RelationKey::GOAL_RESPONSIBLE),
        (TableId::Claims, RelationKey::CLAIM_RESPONSIBLE),
    ] {
        let columns = table.columns();
        let c = columns
            .iter()
            .find(|c| c.key == "responsible")
            .unwrap_or_else(|| panic!("{table:?} has no responsible column"));
        assert_eq!(c.kind, ColumnKind::Links(key), "{table:?}");
        assert_eq!(c.label, ColumnLabel::Catalog("responsible"), "{table:?}");
        assert_eq!(c.width, 160, "{table:?}");
        assert!(c.visible, "{table:?}");
        assert_eq!(columns[2].key, "responsible", "{table:?}");
    }
}
