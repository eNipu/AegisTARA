//! Catalog ID syntax: a small declarative grammar instead of a regex.
//!
//! An ID is an exact prefix followed by 1 to 4 numeric segments separated by
//! `.`. Each segment has a value range and either a fixed digit count
//! (zero-padded) or a free count of 1 to 5 digits with no leading zero. The
//! grammar covers forms such as `4.1`, `CAPEC-123`, and `T1059.001` without
//! a regex dependency.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use super::CatalogScheme;
use crate::error::{MethodologyError, ModelError};

/// Most segments in a catalog ID.
pub const MAX_SEGMENTS: usize = 4;
/// Longest catalog prefix, in bytes.
pub const MAX_PREFIX_LEN: usize = 16;
/// Most digits in one segment.
const MAX_DIGITS: usize = 5;

/// The ID syntax of one catalog scheme.
///
/// Rules, checked by [`Methodology::new`](crate::Methodology::new):
/// - 1 to 4 segments, and `1 <= required <= segments.len()`;
/// - the prefix is 0 to 16 ASCII letters, `-`, or `_` (never a digit, so
///   parsing is unambiguous).
///
/// An ID then has the exact prefix followed by `len` segments joined by
/// `.`, with `required <= len <= segments.len()`.
///
/// ```
/// use aegis_model::{CatalogSyntax, SegmentDef};
///
/// // `N` or `N.M`.
/// let syntax = CatalogSyntax {
///     prefix: "".into(),
///     segments: vec![
///         SegmentDef { min: 1, max: 32, digits: None },
///         SegmentDef { min: 1, max: 99, digits: None },
///     ],
///     required: 1,
/// };
/// assert_eq!(syntax.segments.len(), 2);
/// assert_eq!(syntax.to_string(), "<1..=32>[.<1..=99>]");
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogSyntax {
    /// Exact, case-sensitive text before the first segment.
    pub prefix: Arc<str>,
    /// The segments, 1 to 4.
    pub segments: Vec<SegmentDef>,
    /// How many leading segments an ID must have.
    pub required: u8,
}

impl fmt::Display for CatalogSyntax {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.prefix)?;
        let req = usize::from(self.required);
        for (i, seg) in self.segments.iter().enumerate() {
            if i >= req {
                f.write_str("[.")?;
            } else if i > 0 {
                f.write_str(".")?;
            }
            write!(f, "{seg}")?;
        }
        for _ in req..self.segments.len() {
            f.write_str("]")?;
        }
        Ok(())
    }
}

/// One numeric segment of a catalog ID.
///
/// `min <= max` always. With `digits: Some(w)` the segment is exactly `w`
/// digits, zero-padded, with `1 <= w <= 5` and `max < 10^w`. With `digits:
/// None` it is 1 to 5 digits with no leading zero.
///
/// ```
/// use aegis_model::SegmentDef;
///
/// let technique = SegmentDef { min: 1, max: 9999, digits: Some(4) };
/// assert!(technique.min <= technique.max);
/// assert_eq!(technique.to_string(), "<0001..=9999>");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentDef {
    /// Smallest allowed value.
    pub min: u16,
    /// Largest allowed value.
    pub max: u16,
    /// Fixed digit count, or `None` for a free count without leading zeros.
    pub digits: Option<u8>,
}

impl fmt::Display for SegmentDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let width = self.digits.map_or(0, usize::from);
        write!(f, "<{:0width$}..={:0width$}>", self.min, self.max)
    }
}

impl SegmentDef {
    /// Checks the segment's own rules. Time O(1).
    fn is_valid(self) -> bool {
        self.min <= self.max
    }

    /// Checks a fixed digit count against `max`. Time O(1).
    fn digits_valid(self) -> bool {
        match self.digits {
            None => true,
            Some(w) => (1..=5).contains(&w) && u32::from(self.max) < 10u32.pow(u32::from(w)),
        }
    }

    /// Parses one segment's text. Time O(text length).
    fn parse(self, text: &str) -> Option<u16> {
        let bytes = text.as_bytes();
        if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let width_ok = if let Some(w) = self.digits {
            bytes.len() == usize::from(w)
        } else {
            let leading_zero = bytes.len() > 1 && bytes.first() == Some(&b'0');
            bytes.len() <= MAX_DIGITS && !leading_zero
        };
        if !width_ok {
            return None;
        }
        let value: u32 = text.parse().ok()?;
        let value = u16::try_from(value).ok()?;
        self.accepts(value).then_some(value)
    }

    /// Checks a value against the range. Time O(1).
    fn accepts(self, value: u16) -> bool {
        (self.min..=self.max).contains(&value)
    }
}

impl CatalogSyntax {
    /// Validates the syntax of the scheme named `scheme`. Time O(1).
    pub(crate) fn check(&self, scheme: &Arc<str>) -> Result<(), MethodologyError> {
        let n = self.segments.len();
        if !(1..=MAX_SEGMENTS).contains(&n) {
            return Err(MethodologyError::CatalogSegmentCount {
                scheme: Arc::clone(scheme),
            });
        }
        if self.required == 0 || usize::from(self.required) > n {
            return Err(MethodologyError::CatalogRequired {
                scheme: Arc::clone(scheme),
            });
        }
        for (i, s) in self.segments.iter().enumerate() {
            if !s.is_valid() {
                return Err(MethodologyError::CatalogSegmentRange {
                    scheme: Arc::clone(scheme),
                    segment: i,
                });
            }
            if !s.digits_valid() {
                return Err(MethodologyError::CatalogSegmentDigits {
                    scheme: Arc::clone(scheme),
                    segment: i,
                });
            }
        }
        let prefix_ok = self.prefix.len() <= MAX_PREFIX_LEN
            && self
                .prefix
                .bytes()
                .all(|b| b.is_ascii_alphabetic() || b == b'-' || b == b'_');
        if !prefix_ok {
            return Err(MethodologyError::CatalogPrefix {
                scheme: Arc::clone(scheme),
            });
        }
        Ok(())
    }

    /// Parses `text` as an ID of `scheme`. Time O(text length).
    pub(crate) fn parse(
        &self,
        scheme: CatalogScheme,
        text: &str,
    ) -> Result<CatalogRef, ModelError> {
        let err = || ModelError::InvalidCatalogId {
            scheme,
            syntax: self.clone(),
        };
        let rest = text.strip_prefix(&*self.prefix).ok_or_else(err)?;
        let mut segments = [0u16; MAX_SEGMENTS];
        let mut len = 0usize;
        for part in rest.split('.') {
            let def = self.segments.get(len).ok_or_else(err)?;
            let slot = segments.get_mut(len).ok_or_else(err)?;
            *slot = def.parse(part).ok_or_else(err)?;
            len += 1;
        }
        if len < usize::from(self.required) {
            return Err(err());
        }
        let len = u8::try_from(len).map_err(|_| err())?;
        Ok(CatalogRef {
            scheme,
            len,
            segments,
        })
    }

    /// Checks that a ref satisfies this syntax: its length is allowed, each
    /// used segment is in range and fits the digit count, and unused
    /// segments are 0. Time O(1).
    pub(crate) fn accepts(&self, r: &CatalogRef) -> bool {
        let len = usize::from(r.len);
        if len < usize::from(self.required) || len > self.segments.len() {
            return false;
        }
        let used_ok = r
            .segments()
            .iter()
            .zip(&self.segments)
            .all(|(&v, def)| def.accepts(v));
        let unused_ok = r.segments.iter().skip(len).all(|&v| v == 0);
        used_ok && unused_ok
    }
}

/// A reference to one catalog entry, for example `4.1` in a scheme with the
/// syntax `N` or `N.M`.
///
/// `Copy`, 10 bytes. Built only by
/// [`Methodology::parse_catalog_ref`](crate::Methodology::parse_catalog_ref),
/// so its segments always satisfy the syntax of the scheme it was parsed
/// under. Ordered by scheme, then by the used segments lexicographically, so
/// `4 < 4.1 < 4.2 < 5`.
///
/// ```
/// use aegis_model::Methodology;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let m = Methodology::iso21434_r155()?;
/// let s = m.catalog_scheme("r155-annex5").ok_or("no scheme")?;
/// let (a, b) = (m.parse_catalog_ref(s, "4")?, m.parse_catalog_ref(s, "4.1")?);
/// assert!(a < b);
/// assert_eq!(a.scheme(), s);
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CatalogRef {
    scheme: CatalogScheme,
    len: u8,
    /// Used segments first; unused ones are always 0.
    segments: [u16; MAX_SEGMENTS],
}

impl CatalogRef {
    /// Returns the scheme the ref was parsed under. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let s = m.catalog_scheme("r155-annex5").ok_or("no scheme")?;
    /// assert_eq!(m.parse_catalog_ref(s, "7")?.scheme(), s);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn scheme(&self) -> CatalogScheme {
        self.scheme
    }

    /// Returns the used segment values, in order. Time O(1).
    ///
    /// ```
    /// use aegis_model::Methodology;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let m = Methodology::iso21434_r155()?;
    /// let s = m.catalog_scheme("r155-annex5").ok_or("no scheme")?;
    /// assert_eq!(m.parse_catalog_ref(s, "12.3")?.segments(), [12, 3]);
    /// # Ok(())
    /// # }
    /// ```
    #[must_use]
    pub fn segments(&self) -> &[u16] {
        self.segments
            .get(..usize::from(self.len))
            .unwrap_or(&self.segments)
    }
}

impl Ord for CatalogRef {
    /// Scheme first, then the used segments lexicographically (a prefix
    /// sorts first). Time O(1).
    fn cmp(&self, other: &Self) -> Ordering {
        self.scheme
            .cmp(&other.scheme)
            .then_with(|| self.segments().cmp(other.segments()))
    }
}

impl PartialOrd for CatalogRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for CatalogRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        CatalogRefDisplay {
            syntax: None,
            r: *self,
        }
        .fmt(f)
    }
}

/// Formats a [`CatalogRef`] in its scheme's syntax; returned by
/// [`Methodology::display_catalog_ref`](crate::Methodology::display_catalog_ref).
pub(crate) struct CatalogRefDisplay<'a> {
    /// The scheme's syntax, or `None` if the scheme is unknown.
    pub(crate) syntax: Option<&'a CatalogSyntax>,
    pub(crate) r: CatalogRef,
}

impl fmt::Display for CatalogRefDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(syntax) = self.syntax {
            f.write_str(&syntax.prefix)?;
        }
        for (i, &v) in self.r.segments().iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            let width = self
                .syntax
                .and_then(|s| s.segments.get(i))
                .and_then(|d| d.digits)
                .map_or(0, usize::from);
            write!(f, "{v:0width$}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::methodology::{
        CatalogSchemeDef, ImpactCategoryDef, LevelDef, Methodology, MethodologyDef, PropertyDef,
        ThreatCategoryDef,
    };

    fn seg(min: u16, max: u16, digits: Option<u8>) -> SegmentDef {
        SegmentDef { min, max, digits }
    }

    /// A methodology with a CAPEC-style and an ATT&CK-style scheme.
    fn catalogs() -> Methodology {
        let s = |key: &str, prefix: &str, segments, required| CatalogSchemeDef {
            key: key.into(),
            label: key.into(),
            syntax: CatalogSyntax {
                prefix: prefix.into(),
                segments,
                required,
            },
        };
        Methodology::new(MethodologyDef {
            key: "catalogs".into(),
            version: "1".into(),
            label: "Catalogs".into(),
            element_names: crate::ElementNamesDef::default(),
            impact_categories: vec![ImpactCategoryDef {
                key: "harm".into(),
                label: "Harm".into(),
                levels: vec![
                    LevelDef {
                        key: "low".into(),
                        label: "Low".into(),
                    },
                    LevelDef {
                        key: "high".into(),
                        label: "High".into(),
                    },
                ],
                risk_matrix: vec![
                    vec!["low".into(), "high".into()],
                    vec!["high".into(), "high".into()],
                ],
            }],
            feasibility_levels: vec![
                LevelDef {
                    key: "low".into(),
                    label: "Low".into(),
                },
                LevelDef {
                    key: "high".into(),
                    label: "High".into(),
                },
            ],
            risk_levels: vec![
                LevelDef {
                    key: "low".into(),
                    label: "Low".into(),
                },
                LevelDef {
                    key: "high".into(),
                    label: "High".into(),
                },
            ],
            require_rationale: false,
            attack_potential: None,
            properties: vec![PropertyDef {
                key: "i".into(),
                label: "I".into(),
            }],
            threat_categories: vec![ThreatCategoryDef {
                key: "t".into(),
                label: "T".into(),
                violates: vec!["i".into()],
            }],
            catalog_schemes: vec![
                s("capec", "CAPEC-", vec![seg(1, 9999, None)], 1),
                s(
                    "attack",
                    "T",
                    vec![seg(1, 9999, Some(4)), seg(1, 999, Some(3))],
                    1,
                ),
            ],
        })
        .expect("valid")
    }

    fn round_trips(m: &Methodology, scheme: CatalogScheme, text: &str) {
        let r = m.parse_catalog_ref(scheme, text).expect(text);
        let shown = m.display_catalog_ref(&r).to_string();
        assert_eq!(shown, text);
        assert_eq!(m.parse_catalog_ref(scheme, &shown), Ok(r));
        assert_eq!(m.check_catalog_ref(&r), Ok(()));
    }

    #[test]
    fn catalog_syntax_capec_style() {
        let m = catalogs();
        let capec = m.catalog_scheme("capec").expect("capec");
        let syntax = m.catalog_syntax(capec).expect("syntax").clone();
        assert_eq!(syntax.to_string(), "CAPEC-<1..=9999>");
        for ok in ["CAPEC-123", "CAPEC-1", "CAPEC-9999"] {
            round_trips(&m, capec, ok);
        }
        for bad in [
            "CAPEC-0123",
            "capec-123",
            "CAPEC-0",
            "CAPEC-10000",
            "CAPEC-",
            "CAPEC-1.2",
            "123",
            "CAPEC--1",
        ] {
            assert_eq!(
                m.parse_catalog_ref(capec, bad),
                Err(ModelError::InvalidCatalogId {
                    scheme: capec,
                    syntax: syntax.clone(),
                }),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn catalog_syntax_attack_style() {
        let m = catalogs();
        let attack = m.catalog_scheme("attack").expect("attack");
        let syntax = m.catalog_syntax(attack).expect("syntax").clone();
        assert_eq!(syntax.to_string(), "T<0001..=9999>[.<001..=999>]");
        for ok in ["T1059", "T1059.001", "T0001", "T9999.999"] {
            round_trips(&m, attack, ok);
        }
        let r = m.parse_catalog_ref(attack, "T1059.001").expect("valid");
        assert_eq!(r.segments(), [1059, 1]);
        assert_eq!(r.to_string(), "1059.1");
        for bad in [
            "T1059.1",
            "T59",
            "T10590",
            "T0000",
            "T1059.000",
            "t1059",
            "T1059.",
        ] {
            assert_eq!(
                m.parse_catalog_ref(attack, bad),
                Err(ModelError::InvalidCatalogId {
                    scheme: attack,
                    syntax: syntax.clone(),
                }),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn catalog_syntax_unknown_scheme() {
        let m = catalogs();
        let unknown = CatalogScheme::new(2);
        assert_eq!(
            m.parse_catalog_ref(unknown, "1"),
            Err(ModelError::UnknownCatalogScheme(unknown))
        );
        let r = CatalogRef {
            scheme: unknown,
            len: 2,
            segments: [3, 4, 0, 0],
        };
        assert_eq!(m.display_catalog_ref(&r).to_string(), "3.4");
        assert_eq!(
            m.check_catalog_ref(&r),
            Err(ModelError::UnknownCatalogScheme(unknown))
        );
    }

    #[test]
    fn catalog_syntax_accepts_rejects_foreign_refs() {
        let m = catalogs();
        let attack = m.catalog_scheme("attack").expect("attack");
        let syntax = m.catalog_syntax(attack).expect("syntax");
        let mk = |len, segments| CatalogRef {
            scheme: attack,
            len,
            segments,
        };
        assert!(syntax.accepts(&mk(2, [1059, 1, 0, 0])));
        assert!(!syntax.accepts(&mk(0, [0; 4])), "below required");
        assert!(!syntax.accepts(&mk(3, [1, 1, 1, 0])), "too many segments");
        assert!(!syntax.accepts(&mk(1, [0, 0, 0, 0])), "below min");
        assert!(!syntax.accepts(&mk(2, [1, 1000, 0, 0])), "above max");
        assert!(!syntax.accepts(&mk(1, [1, 5, 0, 0])), "unused not zero");
        assert_eq!(
            m.check_catalog_ref(&mk(1, [0; 4])),
            Err(ModelError::InvalidCatalogId {
                scheme: attack,
                syntax: syntax.clone(),
            })
        );
    }

    #[test]
    fn catalog_syntax_order_is_scheme_then_segments() {
        let m = catalogs();
        let capec = m.catalog_scheme("capec").expect("capec");
        let attack = m.catalog_scheme("attack").expect("attack");
        let p = |s, t| m.parse_catalog_ref(s, t).expect(t);
        assert!(p(capec, "CAPEC-9999") < p(attack, "T0001"));
        assert!(p(attack, "T0001") < p(attack, "T0001.001"));
        assert!(p(attack, "T0001.999") < p(attack, "T0002"));
        assert_eq!(
            p(capec, "CAPEC-5").cmp(&p(capec, "CAPEC-5")),
            Ordering::Equal
        );
    }

    #[test]
    fn catalog_syntax_check_rules() {
        let k: Arc<str> = "s".into();
        let mk = |prefix: &str, segments: Vec<SegmentDef>, required| CatalogSyntax {
            prefix: prefix.into(),
            segments,
            required,
        };
        assert_eq!(mk("", vec![seg(0, 0, None)], 1).check(&k), Ok(()));
        assert!(mk("", vec![seg(0, 9, Some(1))], 1).check(&k).is_ok());
        assert!(mk("", vec![seg(0, 10, Some(1))], 1).check(&k).is_err());
    }
}
