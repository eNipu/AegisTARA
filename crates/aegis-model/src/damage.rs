//! Damage scenarios: what goes wrong, rated per impact category.

use std::sync::Arc;

use crate::methodology::{ImpactCategory, Level};

/// A damage scenario of a [`Project`](crate::Project), rated per impact
/// category of the project's methodology.
///
/// A read-only view. It holds one slot per impact category; `None` means
/// *unrated*, which is kept apart from the lowest level.
///
/// ```
/// use std::sync::Arc;
/// use aegis_model::{DamageId, Methodology, Op, Project};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Arc::new(Methodology::iso21434_r155()?);
/// let mut project = Project::new(Arc::clone(&m));
/// let safety = m.impact_category("safety").ok_or("no category")?;
/// let severe = m.level(safety, "severe").ok_or("no level")?;
/// let id = DamageId::new_v7();
/// project.apply(Op::AddDamageScenario {
///     id,
///     name: "Loss of braking".into(),
///     impacts: vec![(safety, severe)],
/// })?;
/// let (_, _, d) = project.damage_scenarios().next().ok_or("missing")?;
/// assert_eq!(d.impact(safety), Some(severe));
/// let privacy = m.impact_category("privacy").ok_or("no category")?;
/// assert_eq!(d.impact(privacy), None); // unrated
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageScenario {
    pub(crate) name: Arc<str>,
    /// One slot per impact category, in category order.
    pub(crate) impacts: Box<[Option<Level>]>,
}

impl DamageScenario {
    /// Returns the name. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{DamageId, Methodology, Op, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut project = Project::new(Arc::new(Methodology::iso21434_r155()?));
    /// let id = DamageId::new_v7();
    /// project.apply(Op::AddDamageScenario { id, name: "Data leak".into(), impacts: vec![] })?;
    /// let (_, _, d) = project.damage_scenarios().next().ok_or("missing")?;
    /// assert_eq!(d.name(), "Data leak");
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the rating for one category: `None` if unrated or if the
    /// category is not in the methodology. Time O(1).
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{DamageId, Methodology, Op, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// let cat = m.impact_category("financial").ok_or("no category")?;
    /// let low = m.level(cat, "negligible").ok_or("no level")?;
    /// project.apply(Op::AddDamageScenario {
    ///     id: DamageId::new_v7(),
    ///     name: "Fraud".into(),
    ///     impacts: vec![(cat, low)],
    /// })?;
    /// let (_, _, d) = project.damage_scenarios().next().ok_or("missing")?;
    /// assert_eq!(d.impact(cat), Some(low)); // rated lowest, not unrated
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn impact(&self, cat: ImpactCategory) -> Option<Level> {
        self.impacts.get(cat.index()).copied().flatten()
    }

    /// Iterates every impact category of the methodology with its rating,
    /// in category order. Time O(c), c = the number of categories.
    ///
    /// ```
    /// use std::sync::Arc;
    /// use aegis_model::{DamageId, Methodology, Op, Project};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Arc::new(Methodology::iso21434_r155()?);
    /// let mut project = Project::new(Arc::clone(&m));
    /// project.apply(Op::AddDamageScenario {
    ///     id: DamageId::new_v7(),
    ///     name: "Unrated".into(),
    ///     impacts: vec![],
    /// })?;
    /// let (_, _, d) = project.damage_scenarios().next().ok_or("missing")?;
    /// assert_eq!(d.impacts().count(), 4);
    /// assert!(d.impacts().all(|(_, level)| level.is_none()));
    /// # Ok(())
    /// # }
    /// ```
    pub fn impacts(&self) -> impl ExactSizeIterator<Item = (ImpactCategory, Option<Level>)> + '_ {
        self.impacts
            .iter()
            .zip(0..=u8::MAX)
            .map(|(&level, i)| (ImpactCategory::new(i), level))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impacts_and_impact_agree() {
        let l = Level::new;
        let d = DamageScenario {
            name: "d".into(),
            impacts: vec![Some(l(0)), None, Some(l(2))].into_boxed_slice(),
        };
        let all: Vec<_> = d.impacts().collect();
        assert_eq!(all.len(), 3);
        for (cat, level) in all {
            assert_eq!(d.impact(cat), level);
        }
        assert_eq!(d.impact(ImpactCategory::new(0)), Some(l(0)));
        assert_eq!(d.impact(ImpactCategory::new(1)), None);
        assert_eq!(d.impact(ImpactCategory::new(7)), None);
        assert_eq!(d.name(), "d");
    }
}
