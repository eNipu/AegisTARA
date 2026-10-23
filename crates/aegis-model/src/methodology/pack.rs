//! Methodology pack files: format `aegis-methodology-pack` version 1.
//!
//! A pack is one JSON object that holds exactly a [`MethodologyDef`] (see
//! `docs/adr/0011-methodology-pack-files.md`):
//!
//! ```text
//! {"format": "aegis-methodology-pack", "version": 1, "methodology": {...}}
//! ```
//!
//! Reading decodes into private `*Wire` types whose leaves are bounded
//! (`BoundedStr`, `BoundedVec`, shared with the project store): every
//! string and every list is checked against its cap inside the serde
//! visitor, and the whole input against [`MAX_PACK_FILE_BYTES`], before
//! [`Methodology::new`] runs. The wire types then move their `Arc<str>`
//! values into the public `*Def` types, which have no serde derives.
//! Writing serializes borrowed views straight from the definition with the
//! pretty printer, in the declared field order, so the same definition
//! always gives the same bytes (P4).

use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::Arc;

use serde::de::{Deserialize, Deserializer, Visitor};
use serde::ser::{Serialize, SerializeStruct, Serializer};

use super::{
    CatalogSchemeDef, CatalogSyntax, ImpactCategoryDef, LevelDef, MAX_CATALOG_SCHEMES,
    MAX_IMPACT_CATEGORIES, MAX_LEVELS, MAX_PREFIX_LEN, MAX_PROPERTIES, MAX_SEGMENTS,
    MAX_THREAT_CATEGORIES, Methodology, MethodologyDef, PropertyDef, SegmentDef, ThreatCategoryDef,
};
use crate::error::PackError;
use crate::store::wire::{
    self, BoundedStr, BoundedVec, Cap, CappedReader, FORMAT_VERSION, Format, FormatName,
    FormatVersion, Key, VersionText,
};
use crate::store::{Limit, MAX_LABEL_BYTES, MAX_PACK_FILE_BYTES};

/// The pack format name.
const PACK_FORMAT: &str = "aegis-methodology-pack";

// ---------------------------------------------------------------------------
// Caps.
// ---------------------------------------------------------------------------

/// Defines a cap type for a pack list or string.
macro_rules! pack_caps {
    ($($name:ident = $max:expr, $limit:expr;)*) => {$(
        enum $name {}
        impl Cap for $name {
            const MAX: usize = $max;
            const LIMIT: Limit = $limit;
        }
    )*};
}

pack_caps! {
    LabelCap = MAX_LABEL_BYTES, Limit::LABEL;
    PrefixCap = MAX_PREFIX_LEN, Limit::CATALOG_PREFIX;
    ImpactCategoriesCap = MAX_IMPACT_CATEGORIES, Limit::IMPACT_CATEGORIES;
    LevelsCap = MAX_LEVELS, Limit::LEVELS;
    PropertiesCap = MAX_PROPERTIES, Limit::PROPERTIES;
    ThreatCategoriesCap = MAX_THREAT_CATEGORIES, Limit::THREAT_CATEGORIES;
    CatalogSchemesCap = MAX_CATALOG_SCHEMES, Limit::CATALOG_SCHEMES;
    SegmentsCap = MAX_SEGMENTS, Limit::CATALOG_SEGMENTS;
}

type Label = BoundedStr<LabelCap>;

/// The pack format.
enum PackFormat {}
impl FormatName for PackFormat {
    const NAME: &'static str = PACK_FORMAT;
}

// ---------------------------------------------------------------------------
// Objects only.
// ---------------------------------------------------------------------------

/// A struct that must be a JSON object. Serde's derived struct visitor also
/// accepts a JSON array of the field values in order; a pack never uses
/// that form, so it is a shape error ([`PackError::Decode`]).
struct Object<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        T::deserialize(MapOnly(d)).map(Object)
    }
}

/// Forwards to the inner deserializer, but asks for a map where a struct is
/// requested. Time O(1) besides the inner call.
struct MapOnly<D>(D);

impl<'de, D: Deserializer<'de>> Deserializer<'de> for MapOnly<D> {
    type Error = D::Error;

    fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, D::Error> {
        self.0.deserialize_any(v)
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        v: V,
    ) -> Result<V::Value, D::Error> {
        self.0.deserialize_map(v)
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}

// ---------------------------------------------------------------------------
// Owned wire types (reading).
// ---------------------------------------------------------------------------

/// A whole pack. The format fields are checked as soon as they are read.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PackWire {
    #[serde(rename = "format")]
    _format: Format<PackFormat>,
    #[serde(rename = "version")]
    _version: FormatVersion,
    methodology: Object<DefWire>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct DefWire {
    key: Key,
    version: VersionText,
    label: Label,
    impact_categories: BoundedVec<Object<ImpactCategoryWire>, ImpactCategoriesCap>,
    feasibility_levels: BoundedVec<Object<EntryWire>, LevelsCap>,
    risk_levels: BoundedVec<Object<EntryWire>, LevelsCap>,
    properties: BoundedVec<Object<EntryWire>, PropertiesCap>,
    threat_categories: BoundedVec<Object<ThreatCategoryWire>, ThreatCategoriesCap>,
    catalog_schemes: BoundedVec<Object<CatalogSchemeWire>, CatalogSchemesCap>,
}

/// A level or a property: a key and a label.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryWire {
    key: Key,
    label: Label,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ImpactCategoryWire {
    key: Key,
    label: Label,
    levels: BoundedVec<Object<EntryWire>, LevelsCap>,
    risk_matrix: BoundedVec<BoundedVec<Key, LevelsCap>, LevelsCap>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ThreatCategoryWire {
    key: Key,
    label: Label,
    violates: BoundedVec<Key, PropertiesCap>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CatalogSchemeWire {
    key: Key,
    label: Label,
    syntax: Object<SyntaxWire>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SyntaxWire {
    prefix: BoundedStr<PrefixCap>,
    required: u8,
    segments: BoundedVec<Object<SegmentWire>, SegmentsCap>,
}

/// One segment. `digits` may be missing or `null`; both mean `None`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SegmentWire {
    min: u16,
    max: u16,
    #[serde(default)]
    digits: Option<u8>,
}

/// Moves the decoded values into a definition: no string is copied, only
/// the `Arc<str>` handles move. Time O(m), m = vocabulary entries.
impl From<DefWire> for MethodologyDef {
    fn from(w: DefWire) -> Self {
        let entry = |e: Object<EntryWire>| (e.0.key.0, e.0.label.0);
        MethodologyDef {
            key: w.key.0,
            version: w.version.0,
            label: w.label.0,
            impact_categories: w
                .impact_categories
                .0
                .into_iter()
                .map(|Object(c)| ImpactCategoryDef {
                    key: c.key.0,
                    label: c.label.0,
                    levels: c
                        .levels
                        .0
                        .into_iter()
                        .map(entry)
                        .map(|(key, label)| LevelDef { key, label })
                        .collect(),
                    risk_matrix: c
                        .risk_matrix
                        .0
                        .into_iter()
                        .map(|row| row.0.into_iter().map(|k| k.0).collect())
                        .collect(),
                })
                .collect(),
            feasibility_levels: w
                .feasibility_levels
                .0
                .into_iter()
                .map(entry)
                .map(|(key, label)| LevelDef { key, label })
                .collect(),
            risk_levels: w
                .risk_levels
                .0
                .into_iter()
                .map(entry)
                .map(|(key, label)| LevelDef { key, label })
                .collect(),
            properties: w
                .properties
                .0
                .into_iter()
                .map(entry)
                .map(|(key, label)| PropertyDef { key, label })
                .collect(),
            threat_categories: w
                .threat_categories
                .0
                .into_iter()
                .map(|Object(t)| ThreatCategoryDef {
                    key: t.key.0,
                    label: t.label.0,
                    violates: t.violates.0.into_iter().map(|k| k.0).collect(),
                })
                .collect(),
            catalog_schemes: w
                .catalog_schemes
                .0
                .into_iter()
                .map(|Object(s)| CatalogSchemeDef {
                    key: s.key.0,
                    label: s.label.0,
                    syntax: CatalogSyntax {
                        prefix: s.syntax.0.prefix.0,
                        segments: s
                            .syntax
                            .0
                            .segments
                            .0
                            .into_iter()
                            .map(|Object(g)| SegmentDef {
                                min: g.min,
                                max: g.max,
                                digits: g.digits,
                            })
                            .collect(),
                        required: s.syntax.0.required,
                    },
                })
                .collect(),
        }
    }
}

// ---------------------------------------------------------------------------
// Borrowed views (writing).
// ---------------------------------------------------------------------------

/// The whole pack, borrowed from the definition.
#[derive(serde::Serialize)]
struct PackView<'a> {
    format: &'static str,
    version: u32,
    methodology: View<'a, MethodologyDef>,
}

/// A borrowed part of a definition, serialized in the pack's field order.
struct View<'a, T: ?Sized>(&'a T);

impl<T> Serialize for View<'_, [T]>
where
    for<'b> View<'b, T>: Serialize,
{
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(View))
    }
}

impl Serialize for View<'_, [Arc<str>]> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.iter().map(|k| &**k))
    }
}

impl Serialize for View<'_, Vec<Arc<str>>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        View(self.0.as_slice()).serialize(s)
    }
}

impl Serialize for View<'_, MethodologyDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let d = self.0;
        let mut st = s.serialize_struct("methodology", 9)?;
        st.serialize_field("key", &*d.key)?;
        st.serialize_field("version", &*d.version)?;
        st.serialize_field("label", &*d.label)?;
        st.serialize_field("impact_categories", &View(d.impact_categories.as_slice()))?;
        st.serialize_field("feasibility_levels", &View(d.feasibility_levels.as_slice()))?;
        st.serialize_field("risk_levels", &View(d.risk_levels.as_slice()))?;
        st.serialize_field("properties", &View(d.properties.as_slice()))?;
        st.serialize_field("threat_categories", &View(d.threat_categories.as_slice()))?;
        st.serialize_field("catalog_schemes", &View(d.catalog_schemes.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, ImpactCategoryDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let c = self.0;
        let mut st = s.serialize_struct("impact_category", 4)?;
        st.serialize_field("key", &*c.key)?;
        st.serialize_field("label", &*c.label)?;
        st.serialize_field("levels", &View(c.levels.as_slice()))?;
        st.serialize_field("risk_matrix", &View(c.risk_matrix.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, LevelDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        entry(s, "level", &self.0.key, &self.0.label)
    }
}

impl Serialize for View<'_, PropertyDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        entry(s, "property", &self.0.key, &self.0.label)
    }
}

/// `{"key": ..., "label": ...}`.
fn entry<S: Serializer>(
    s: S,
    name: &'static str,
    key: &str,
    label: &str,
) -> Result<S::Ok, S::Error> {
    let mut st = s.serialize_struct(name, 2)?;
    st.serialize_field("key", key)?;
    st.serialize_field("label", label)?;
    st.end()
}

impl Serialize for View<'_, ThreatCategoryDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let t = self.0;
        let mut st = s.serialize_struct("threat_category", 3)?;
        st.serialize_field("key", &*t.key)?;
        st.serialize_field("label", &*t.label)?;
        st.serialize_field("violates", &View(t.violates.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, CatalogSchemeDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let c = self.0;
        let mut st = s.serialize_struct("catalog_scheme", 3)?;
        st.serialize_field("key", &*c.key)?;
        st.serialize_field("label", &*c.label)?;
        st.serialize_field("syntax", &View(&c.syntax))?;
        st.end()
    }
}

impl Serialize for View<'_, CatalogSyntax> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let x = self.0;
        let mut st = s.serialize_struct("syntax", 3)?;
        st.serialize_field("prefix", &*x.prefix)?;
        st.serialize_field("required", &x.required)?;
        st.serialize_field("segments", &View(x.segments.as_slice()))?;
        st.end()
    }
}

impl Serialize for View<'_, SegmentDef> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let g = self.0;
        let mut st = s.serialize_struct("segment", 3)?;
        st.serialize_field("min", &g.min)?;
        st.serialize_field("max", &g.max)?;
        match g.digits {
            Some(w) => st.serialize_field("digits", &w)?,
            None => st.skip_field("digits")?,
        }
        st.end()
    }
}

// ---------------------------------------------------------------------------
// Public API.
// ---------------------------------------------------------------------------

impl MethodologyDef {
    /// Decodes an `aegis-methodology-pack` v1 file: the format check, the
    /// decode caps, and the strict shape. It does not validate the
    /// definition; [`Methodology::read_pack`] does both.
    ///
    /// The input is read through a buffer, never more than
    /// [`MAX_PACK_FILE_BYTES`] + 1 bytes of it. Every string and list is
    /// checked against its cap ([`Limit`]) while it is decoded. Unknown,
    /// missing, repeated, and mistyped fields, numbers outside their
    /// field's range, and trailing data are errors. `digits` on a segment
    /// may be missing or `null` (both `None`); every other field is
    /// required.
    ///
    /// Time O(B), B = input bytes. Space O(B): at most 4 MiB of input, and
    /// at most about 1.4 MB of definition at the caps.
    ///
    /// # Errors
    ///
    /// [`PackError::Io`] if reading fails; [`PackError::LimitExceeded`] if
    /// a cap breaks; [`PackError::UnsupportedFormat`] for a wrong format
    /// name or version; [`PackError::Decode`] for bad JSON or a wrong
    /// shape.
    ///
    /// ```
    /// use aegis_model::{Methodology, MethodologyDef};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let text = r#"{
    ///   "format": "aegis-methodology-pack",
    ///   "version": 1,
    ///   "methodology": {
    ///     "key": "tiny", "version": "1", "label": "Tiny",
    ///     "impact_categories": [{ "key": "harm", "label": "Harm", "levels": [
    ///       { "key": "low", "label": "Low" }, { "key": "high", "label": "High" }],
    ///       "risk_matrix": [["r1", "r2"], ["r2", "r2"]] }],
    ///     "feasibility_levels": [
    ///       { "key": "low", "label": "Low" }, { "key": "high", "label": "High" }],
    ///     "risk_levels": [
    ///       { "key": "r1", "label": "Low" }, { "key": "r2", "label": "High" }],
    ///     "properties": [{ "key": "integrity", "label": "Integrity" }],
    ///     "threat_categories": [{ "key": "corruption", "label": "Corruption",
    ///                             "violates": ["integrity"] }],
    ///     "catalog_schemes": []
    ///   }
    /// }"#;
    /// let def = MethodologyDef::read_pack(text.as_bytes())?;
    /// assert_eq!(&*def.key, "tiny");
    /// assert_eq!(Methodology::new(def)?.properties().count(), 1);
    /// # Ok(())
    /// # }
    /// ```
    pub fn read_pack<R: Read>(r: R) -> Result<MethodologyDef, PackError> {
        let mut input = CappedReader::new(r, MAX_PACK_FILE_BYTES);
        let decoded = wire::decode_pack(|| {
            let mut de = serde_json::Deserializer::from_reader(BufReader::new(&mut input));
            let pack = Object::<PackWire>::deserialize(&mut de)?;
            de.end()?;
            Ok(pack)
        });
        if input.exceeded() {
            return Err(PackError::LimitExceeded(Limit::PACK_FILE));
        }
        Ok(decoded?.0.methodology.0.into())
    }

    /// Writes the definition as canonical `aegis-methodology-pack` v1 JSON:
    /// the pretty printer (2-space indent), the format's field order,
    /// `digits` left out when `None`, non-ASCII as raw UTF-8, and a final
    /// `\n`. The same definition always gives the same bytes (P4). It does
    /// not validate the definition, but every definition that
    /// [`Methodology::new`] accepts reads back equal.
    ///
    /// Writes go through an internal `BufWriter`, flushed before return.
    /// Time O(m + text), m = vocabulary entries; it serializes straight
    /// from the definition, with no copy (extra space O(1) besides the
    /// buffer).
    ///
    /// # Errors
    ///
    /// Only [`PackError::Io`], if writing fails.
    ///
    /// ```
    /// use aegis_model::{Methodology, MethodologyDef};
    ///
    /// # fn main() -> Result<(), aegis_model::PackError> {
    /// let builtin = include_str!("../../packs/iso21434-r155.json");
    /// let def = MethodologyDef::read_pack(builtin.as_bytes())?;
    /// let mut out = Vec::new();
    /// def.write_pack(&mut out)?;
    /// assert_eq!(out, builtin.as_bytes());
    /// # Ok(())
    /// # }
    /// ```
    pub fn write_pack<W: Write>(&self, w: W) -> Result<(), PackError> {
        let view = PackView {
            format: PACK_FORMAT,
            version: FORMAT_VERSION,
            methodology: View(self),
        };
        let mut out = BufWriter::new(w);
        serde_json::to_writer_pretty(&mut out, &view).map_err(std::io::Error::from)?;
        out.write_all(b"\n")?;
        out.flush()?;
        Ok(())
    }
}

impl Methodology {
    /// Reads a methodology pack and validates it:
    /// [`MethodologyDef::read_pack`], then [`Methodology::new`].
    ///
    /// Time O(B + m log m + text), B = input bytes, m = vocabulary entries
    /// (at most 705). Space O(B).
    ///
    /// # Errors
    ///
    /// Every error of [`MethodologyDef::read_pack`], and
    /// [`PackError::Methodology`] if the definition breaks a methodology
    /// rule.
    ///
    /// ```
    /// use aegis_model::{Methodology, MethodologyError, PackError};
    ///
    /// let text = r#"{"format": "aegis-methodology-pack", "version": 1, "methodology": {
    ///   "key": "tiny", "version": "1", "label": "Tiny", "impact_categories": [],
    ///   "feasibility_levels": [], "risk_levels": [],
    ///   "properties": [], "threat_categories": [], "catalog_schemes": []}}"#;
    /// let err = Methodology::read_pack(text.as_bytes()).err();
    /// assert!(matches!(err, Some(PackError::Methodology(MethodologyError::NoImpactCategory))));
    /// ```
    pub fn read_pack<R: Read>(r: R) -> Result<Methodology, PackError> {
        Ok(Methodology::new(MethodologyDef::read_pack(r)?)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r#"{"format":"aegis-methodology-pack","version":1,"methodology":"#;

    fn tiny(methodology: &str) -> String {
        format!("{HEAD}{methodology}}}")
    }

    #[test]
    fn object_rejects_array_form() {
        let text = format!(r#"{HEAD}["k","1","L",[],[],[],[],[],[]]}}"#);
        assert!(matches!(
            MethodologyDef::read_pack(text.as_bytes()),
            Err(PackError::Decode { line: 1, .. })
        ));
        let whole = r#"["aegis-methodology-pack",1,{}]"#;
        assert!(matches!(
            MethodologyDef::read_pack(whole.as_bytes()),
            Err(PackError::Decode { .. })
        ));
    }

    #[test]
    fn map_only_forwards_other_requests() {
        let mut de = serde_json::Deserializer::from_str("7");
        assert_eq!(u8::deserialize(MapOnly(&mut de)).ok(), Some(7));
        let mut de = serde_json::Deserializer::from_str(r#"{"a":1}"#);
        let any = serde_json::Value::deserialize(MapOnly(&mut de)).ok();
        assert_eq!(any.map(|v| v.to_string()), Some(r#"{"a":1}"#.into()));
    }

    #[test]
    fn digits_missing_null_or_set() {
        let seg = |digits: &str| {
            let text = tiny(&format!(
                r#"{{"key":"k","version":"1","label":"L","impact_categories":[],"feasibility_levels":[],"risk_levels":[],"properties":[],"threat_categories":[],"catalog_schemes":[{{"key":"s","label":"S","syntax":{{"prefix":"","required":1,"segments":[{{"min":1,"max":9{digits}}}]}}}}]}}"#
            ));
            MethodologyDef::read_pack(text.as_bytes())
                .ok()
                .and_then(|d| d.catalog_schemes.first().map(|s| s.syntax.segments.clone()))
        };
        let none = Some(vec![SegmentDef {
            min: 1,
            max: 9,
            digits: None,
        }]);
        assert_eq!(seg(""), none);
        assert_eq!(seg(r#","digits":null"#), none);
        assert_eq!(
            seg(r#","digits":2"#),
            Some(vec![SegmentDef {
                min: 1,
                max: 9,
                digits: Some(2)
            }])
        );
    }

    #[test]
    fn write_pack_reports_io_errors() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk full"))
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Err(std::io::Error::other("disk full"))
            }
        }
        let def = crate::testkit::second_def();
        assert!(matches!(def.write_pack(Broken), Err(PackError::Io(_))));
    }

    #[test]
    fn read_pack_reports_io_errors() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("disk gone"))
            }
        }
        assert!(matches!(
            MethodologyDef::read_pack(Broken),
            Err(PackError::Io(_))
        ));
    }
}
