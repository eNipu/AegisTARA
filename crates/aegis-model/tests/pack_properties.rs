//! Property tests for methodology packs (job 012): every valid definition
//! round-trips through `write_pack` and `read_pack` (AC7), and hostile
//! input never panics (AC12).

mod common;

use std::sync::Arc;

use aegis_model::{
    CatalogSchemeDef, CatalogSyntax, ImpactCategoryDef, LevelDef, Methodology, MethodologyDef,
    PropertyDef, SegmentDef, ThreatCategoryDef,
};
use common::{max_def, pack_bytes, second_def};
use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::subsequence;

// ---------------------------------------------------------------------------
// Valid definitions.
// ---------------------------------------------------------------------------

/// A count in `min..=max`, often at an end of the range.
fn count(min: usize, max: usize) -> impl Strategy<Value = usize> {
    prop_oneof![
        3 => min..=max.min(min + 3),
        1 => Just(max),
        2 => min..=max,
    ]
}

/// A label char: ASCII, controls (escaped in JSON), quotes and
/// backslashes, and 2-, 3-, and 4-byte chars.
fn label_char() -> impl Strategy<Value = char> {
    prop_oneof![
        4 => proptest::char::range(' ', '~'),
        2 => proptest::char::range('\u{0}', '\u{1f}'),
        1 => Just('"'),
        1 => Just('\\'),
        1 => Just('\u{7f}'),
        1 => Just('é'),
        1 => Just('中'),
        1 => Just('\u{1f600}'),
        1 => any::<char>(),
    ]
}

/// `first`, then `len` chars that repeat `pattern`. Drawing a short
/// pattern and a length, instead of every char, keeps generation of a
/// maximal definition (689 labels) cheap while still reaching every
/// length and every kind of char.
fn cycled(first: char, pattern: &[char], len: usize) -> String {
    let mut s = String::from(first);
    s.extend(pattern.iter().copied().cycle().take(len));
    s
}

/// A text of `first` plus 0 to `max_tail` chars, often at the maximum.
fn text(first: char, max_tail: usize) -> impl Strategy<Value = Arc<str>> {
    (
        vec(label_char(), 1..=6),
        prop_oneof![2 => 0..=max_tail, 1 => Just(max_tail)],
    )
        .prop_map(move |(pattern, len)| cycled(first, &pattern, len).into())
}

/// A label of 1 to 256 chars that is never blank.
fn label() -> impl Strategy<Value = Arc<str>> {
    text('x', 255)
}

/// A methodology version: 1 to 64 chars, never blank.
fn version() -> impl Strategy<Value = Arc<str>> {
    text('v', 63)
}

/// Key chars after the first.
const KEY_CHARS: [char; 8] = ['a', 'k', 'z', '0', '5', '9', '_', '-'];

/// Random key text after a unique stem.
fn key_tail() -> impl Strategy<Value = String> {
    (
        vec(proptest::sample::select(&KEY_CHARS[..]), 1..=3),
        0usize..=60,
    )
        .prop_map(|(pattern, len)| pattern.iter().copied().cycle().take(len).collect())
}

/// A key unique within its vocabulary: `{tag}{i}-` then a tail, cut to 64
/// bytes. The stem differs between any two `i`, so keys never collide.
fn key(tag: &str, i: usize, tail: &str) -> Arc<str> {
    let mut k = format!("{tag}{i}-{tail}");
    k.truncate(64);
    k.into()
}

/// One catalog segment: `digits` absent or 1 to 5, `max` fits it, `min <= max`.
fn segment() -> impl Strategy<Value = SegmentDef> {
    proptest::option::of(1u8..=5).prop_flat_map(|digits| {
        let top = match digits {
            Some(w) => u16::try_from(10u32.pow(u32::from(w)) - 1).unwrap_or(u16::MAX),
            None => u16::MAX,
        };
        (0..=top).prop_flat_map(move |max| {
            (0..=max).prop_map(move |min| SegmentDef { min, max, digits })
        })
    })
}

fn syntax() -> impl Strategy<Value = CatalogSyntax> {
    (
        "[A-Za-z_-]{0,16}",
        vec(segment(), 1..=4).prop_flat_map(|segs| {
            let n = u8::try_from(segs.len()).unwrap_or(4);
            (Just(segs), 1..=n)
        }),
    )
        .prop_map(|(prefix, (segments, required))| CatalogSyntax {
            prefix: prefix.into(),
            segments,
            required,
        })
}

fn entries(min: usize, max: usize) -> impl Strategy<Value = Vec<(String, Arc<str>)>> {
    count(min, max).prop_flat_map(|n| vec((key_tail(), label()), n))
}

/// A valid definition: every vocabulary from 1 entry (0 schemes) up to its
/// maximum, random keys, labels, and syntaxes.
fn valid_def() -> impl Strategy<Value = MethodologyDef> {
    let impacts = count(1, 32).prop_flat_map(|n| vec((key_tail(), label(), entries(2, 16)), n));
    let schemes = count(0, 16).prop_flat_map(|n| vec((key_tail(), label(), syntax()), n));
    let properties_and_threats = entries(1, 64).prop_flat_map(|props| {
        let keys: Vec<Arc<str>> = props
            .iter()
            .enumerate()
            .map(|(i, (tail, _))| key("p", i, tail))
            .collect();
        let n = keys.len();
        let violates = subsequence(keys.clone(), 1..=n).prop_shuffle();
        let threats =
            count(1, 64).prop_flat_map(move |t| vec((key_tail(), label(), violates.clone()), t));
        (Just(props), threats)
    });
    (
        key_tail(),
        version(),
        label(),
        impacts,
        properties_and_threats,
        schemes,
        entries(2, 16),
    )
        .prop_map(
            |(mkey, version, label, impacts, (props, threats), schemes, feasibility)| {
                MethodologyDef {
                    key: key("m", 0, &mkey),
                    version,
                    label,
                    impact_categories: impacts
                        .into_iter()
                        .enumerate()
                        .map(|(i, (tail, label, levels))| ImpactCategoryDef {
                            key: key("c", i, &tail),
                            label,
                            levels: levels
                                .into_iter()
                                .enumerate()
                                .map(|(l, (tail, label))| LevelDef {
                                    key: key("l", l, &tail),
                                    label,
                                })
                                .collect(),
                        })
                        .collect(),
                    properties: props
                        .into_iter()
                        .enumerate()
                        .map(|(i, (tail, label))| PropertyDef {
                            key: key("p", i, &tail),
                            label,
                        })
                        .collect(),
                    threat_categories: threats
                        .into_iter()
                        .enumerate()
                        .map(|(i, (tail, label, violates))| ThreatCategoryDef {
                            key: key("t", i, &tail),
                            label,
                            violates,
                        })
                        .collect(),
                    catalog_schemes: schemes
                        .into_iter()
                        .enumerate()
                        .map(|(i, (tail, label, syntax))| CatalogSchemeDef {
                            key: key("s", i, &tail),
                            label,
                            syntax,
                        })
                        .collect(),
                    feasibility_levels: feasibility
                        .into_iter()
                        .enumerate()
                        .map(|(l, (tail, label))| LevelDef {
                            key: key("f", l, &tail),
                            label,
                        })
                        .collect(),
                }
            },
        )
}

/// Checks the round trip of one valid definition.
fn check_round_trip(def: &MethodologyDef) -> Result<(), TestCaseError> {
    let built = Methodology::new(def.clone());
    prop_assert!(built.is_ok(), "generated an invalid def: {:?}", built.err());
    let bytes = pack_bytes(def);
    prop_assert_eq!(&pack_bytes(def), &bytes, "writing twice differs");
    let back = MethodologyDef::read_pack(bytes.as_slice());
    prop_assert!(back.is_ok(), "read_pack failed: {:?}", back.as_ref().err());
    prop_assert_eq!(back.ok(), Some(def.clone()));
    let loaded = Methodology::read_pack(bytes.as_slice());
    prop_assert_eq!(loaded.ok(), built.ok());
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn round_trip(def in valid_def()) {
        check_round_trip(&def)?;
    }
}

#[test]
fn round_trip_fixed_defs() {
    for def in [second_def(), max_def()] {
        if let Err(e) = check_round_trip(&def) {
            panic!("{e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Hostile input.
// ---------------------------------------------------------------------------

/// Reads `bytes` both ways; never panics. If the definition is valid, it
/// must also round-trip.
fn read_hostile(bytes: &[u8]) -> Result<(), TestCaseError> {
    let loaded = Methodology::read_pack(bytes);
    if let Ok(def) = MethodologyDef::read_pack(bytes) {
        let valid = Methodology::new(def.clone());
        prop_assert_eq!(valid.is_ok(), loaded.is_ok());
        if valid.is_ok() {
            check_round_trip(&def)?;
        }
    } else {
        prop_assert!(loaded.is_err());
    }
    Ok(())
}

/// The packs the hostile tests start from.
fn sample_packs() -> Vec<Vec<u8>> {
    vec![
        include_bytes!("../packs/iso21434-r155.json").to_vec(),
        include_bytes!("fixtures/packs/med-device-demo.json").to_vec(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn hostile_arbitrary_bytes(bytes in vec(any::<u8>(), 0..2048)) {
        read_hostile(&bytes)?;
    }

    #[test]
    fn hostile_truncated_pack(which in 0usize..2, cut in any::<prop::sample::Index>()) {
        let pack = &sample_packs()[which];
        let end = cut.index(pack.len());
        let r = Methodology::read_pack(&pack[..end]);
        prop_assert!(r.is_err(), "a cut pack must not load");
    }

    #[test]
    fn hostile_flipped_bytes(
        which in 0usize..2,
        flips in vec((any::<prop::sample::Index>(), any::<u8>()), 1..8),
    ) {
        let mut pack = sample_packs()[which].clone();
        for (at, mask) in flips {
            let i = at.index(pack.len());
            pack[i] ^= mask;
        }
        read_hostile(&pack)?;
    }

}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn hostile_generated_pack_cut_or_flipped(
        def in valid_def(),
        cut in any::<prop::sample::Index>(),
        flip in any::<prop::sample::Index>(),
        mask in 1u8..,
    ) {
        let mut bytes = pack_bytes(&def);
        let i = flip.index(bytes.len());
        bytes[i] ^= mask;
        read_hostile(&bytes)?;
        let end = cut.index(bytes.len());
        read_hostile(&bytes[..end])?;
    }
}
