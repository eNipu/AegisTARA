//! Headless view model and edit helpers for the Attacker Profile page (job 137).

use aegis_model::{AttackFilter, FactorLevel, FeasibilityFactor, Project, ScalabilityFilter};

use crate::commands::Edit;
use crate::configuration::ExcludedCounts;
use crate::i18n::Catalog;
use crate::workspace::Workspace;

/// One factor-level chip within a [`FactorColumn`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactorChip {
    pub factor: FeasibilityFactor,
    pub level: FactorLevel,
    pub key: String,
    pub label: String,
    pub points: u16,
    pub included: bool,
}

/// One attack-potential factor column on the Attacker Profile page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactorColumn {
    pub factor: FeasibilityFactor,
    pub key: String,
    pub label: String,
    pub all_selected: bool,
    pub levels: Vec<FactorChip>,
}

/// One threat-scalability mode option on the Attacker Profile page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScalabilityOption {
    pub mode: ScalabilityFilter,
    pub label: String,
    pub selected: bool,
}

/// Headless view model for the Attacker Profile page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AttackerProfileView {
    pub factors: Vec<FactorColumn>,
    pub scalability_options: Vec<ScalabilityOption>,
    pub scalability: ScalabilityFilter,
    pub is_default: bool,
    pub excluded: ExcludedCounts,
    pub excluded_total: usize,
    pub explainer: String,
}

/// Builds the Attacker Profile view model for `ws` in `catalog`'s language.
#[must_use]
pub fn attacker_profile_view(ws: &Workspace, catalog: &Catalog) -> AttackerProfileView {
    let project = ws.project();
    let m = project.methodology();
    let lang = catalog.language().code();
    let filter = project.attack_filter();

    let factors = m
        .feasibility_factors()
        .map(|factor| {
            let key = m
                .feasibility_factor_key(factor)
                .unwrap_or_default()
                .to_owned();
            let label = m
                .feasibility_factor_label_for_lang(factor, lang)
                .unwrap_or(&key)
                .to_owned();
            let levels: Vec<FactorChip> = m
                .factor_levels(factor)
                .map(|level| {
                    let l_key = m
                        .factor_level_key(factor, level)
                        .unwrap_or_default()
                        .to_owned();
                    let l_label = m
                        .factor_level_label_for_lang(factor, level, lang)
                        .unwrap_or(&l_key)
                        .to_owned();
                    let points = m.factor_level_points(factor, level).unwrap_or(0);
                    FactorChip {
                        factor,
                        level,
                        key: l_key,
                        label: l_label,
                        points,
                        included: filter.allows(factor, level),
                    }
                })
                .collect();
            let all_selected = levels.iter().all(|c| c.included);
            FactorColumn {
                factor,
                key,
                label,
                all_selected,
                levels,
            }
        })
        .collect();

    let scalability = filter.scalability();
    let scalability_options = [
        (ScalabilityFilter::Any, "attacker-profile-scalability-any"),
        (
            ScalabilityFilter::ScaledOnly,
            "attacker-profile-scalability-scaled",
        ),
        (
            ScalabilityFilter::UnscaledOnly,
            "attacker-profile-scalability-unscaled",
        ),
    ]
    .into_iter()
    .map(|(mode, key)| ScalabilityOption {
        mode,
        label: catalog.tr(key),
        selected: scalability == mode,
    })
    .collect();

    let excluded = crate::configuration::excluded_counts(project);
    let excluded_total = excluded.elements + excluded.steps;
    let explainer = if excluded_total == 0 {
        catalog.tr("attacker-profile-explainer-none")
    } else {
        catalog.tr_with(
            "attacker-profile-explainer",
            &[("n", &excluded_total.to_string())],
        )
    };

    AttackerProfileView {
        factors,
        scalability_options,
        scalability,
        is_default: filter.is_default(),
        excluded,
        excluded_total,
        explainer,
    }
}

/// Returns an [`Edit`] that toggles inclusion of `(factor, level)`.
#[must_use]
pub fn toggle_chip_edit(project: &Project, factor: FeasibilityFactor, level: FactorLevel) -> Edit {
    Edit::SetAttackFilter {
        filter: project.attack_filter().with_toggled(factor, level),
    }
}

/// Returns an [`Edit`] that includes every level of `factor`.
#[must_use]
pub fn select_all_factor_edit(project: &Project, factor: FeasibilityFactor) -> Edit {
    Edit::SetAttackFilter {
        filter: project.attack_filter().with_factor_included(factor),
    }
}

/// Returns an [`Edit`] that sets the threat scalability filter.
#[must_use]
pub fn scalability_edit(project: &Project, scalability: ScalabilityFilter) -> Edit {
    Edit::SetAttackFilter {
        filter: project.attack_filter().with_scalability(scalability),
    }
}

/// Returns an [`Edit`] that resets the attacker profile to default.
#[must_use]
pub fn reset_edit() -> Edit {
    Edit::SetAttackFilter {
        filter: AttackFilter::default(),
    }
}
