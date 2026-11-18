//! Client core. No UI toolkit.
#![forbid(unsafe_code)]
pub mod audit;
pub mod combined;
pub mod command_table;
pub mod commands;
pub mod connection;
pub mod error;
pub mod excel;
pub mod explorer;
pub mod find;
pub mod folder;
pub mod i18n;
pub mod library;
pub mod outline;
pub mod pages;
pub mod palette;
pub mod picker;
pub mod provider;
pub mod relations;
pub mod report;
pub mod sample;
pub mod session;
pub mod settings;
pub mod signin;
pub mod state;
pub mod suggest;
pub mod systems;
pub mod table;
pub mod time;
pub mod top_paths;
pub mod transport;
mod tree_diff;
pub mod undo_label;
pub mod updates;
pub mod view;
pub mod workspace;

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aegis_model::{GoalId, Methodology, Op, PartyId, PartyKind, Provenance};

    use crate::commands::Edit;
    use crate::picker::MultiLinkPicker;
    use crate::relations::{RelationKey, related};
    use crate::view::Selection;
    use crate::workspace::{Origin, Workspace};

    fn saved(ws: &Workspace) -> Vec<u8> {
        let mut out = Vec::new();
        ws.project().save(&mut out).expect("save");
        out
    }

    /// The picker on the new row emits the typed link edit, and applying then
    /// undoing it restores the saved bytes exactly (L06).
    #[test]
    fn picker_toggles_a_goal_party_and_undo_restores_bytes() {
        let mut ws = Workspace::empty(Arc::new(Methodology::iso21434_r155().expect("builtin")));
        let (goal, party) = (GoalId::new_v7(), PartyId::new_v7());
        let prov = || Provenance::human("tester", 1_792_956_600_000);
        ws.apply(
            vec![
                Op::AddCybersecurityGoal {
                    id: goal,
                    name: "Sign firmware".into(),
                    controls: vec![],
                    provenance: prov(),
                },
                Op::AddParty {
                    id: party,
                    name: "Bosch".into(),
                    kind: PartyKind::Supplier,
                    provenance: prov(),
                },
            ],
            Origin::Edit,
        )
        .expect("applies");
        let before = saved(&ws);

        let picker = MultiLinkPicker::open(
            ws.project(),
            RelationKey::GOAL_RESPONSIBLE,
            Selection::Goal(goal),
        );
        assert_eq!(picker.options().len(), 1);
        let edit = picker.toggle_active().expect("toggle");
        assert_eq!(
            edit,
            Edit::LinkGoalParty {
                goal,
                party,
                linked: true
            }
        );

        let ops = edit
            .to_ops(ws.project(), "tester", 1_792_956_600_000)
            .expect("ops");
        ws.apply(ops, Origin::Edit).expect("applies");
        assert_eq!(
            related(
                ws.project(),
                RelationKey::GOAL_RESPONSIBLE,
                Selection::Goal(goal)
            ),
            vec![Selection::Party(party)]
        );
        assert_ne!(saved(&ws), before, "the link was not persisted");

        ws.undo().expect("undo");
        assert_eq!(saved(&ws), before, "undo did not restore the saved bytes");
    }
}
