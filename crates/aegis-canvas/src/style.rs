//! Style tokens.
//!
//! The canvas never emits colors. It emits **tokens** ([`Paint`]) and each
//! adapter resolves them: the SVG adapter through an [`SvgTheme`], a screen
//! toolkit through its own palette. That keeps the screen and the report
//! drawing the same picture, lets dark mode be a palette swap, and keeps
//! the rating colors in one place (`docs/design/aegis-design.html`: the
//! Okabe-Ito based set that stays distinguishable for color-blind users).
//!
//! [`SvgTheme`]: crate::svg::SvgTheme

/// How a node's rating looks. Four colored buckets plus "not rated".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tone {
    /// No rating yet.
    #[default]
    Unrated,
    /// Lowest bucket.
    VeryLow,
    /// Second bucket.
    Low,
    /// Third bucket.
    Medium,
    /// Highest bucket.
    High,
}

impl Tone {
    /// Maps the position of a level in an ordered scale of `count` levels
    /// (`0` is the lowest) onto the four buckets, spreading them evenly.
    /// Time O(1).
    ///
    /// ```
    /// use aegis_canvas::Tone;
    /// assert_eq!(Tone::from_level(0, 5), Tone::VeryLow);
    /// assert_eq!(Tone::from_level(4, 5), Tone::High);
    /// assert_eq!(Tone::from_level(0, 1), Tone::Medium);
    /// ```
    #[must_use]
    pub fn from_level(index: usize, count: usize) -> Tone {
        if count <= 1 {
            return Tone::Medium;
        }
        let index = index.min(count - 1);
        // Bucket = round(index * 3 / (count - 1)): the lowest level is the
        // lowest bucket, the highest level is the highest bucket, and the
        // levels between are spread as evenly as integers allow.
        let last = count - 1;
        match (index * 6 + last) / (2 * last) {
            0 => Tone::VeryLow,
            1 => Tone::Low,
            2 => Tone::Medium,
            _ => Tone::High,
        }
    }
}

/// What a shape is filled or stroked with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Paint {
    /// The canvas background ("drafting film").
    Sheet,
    /// Node and panel surfaces.
    Panel,
    /// Primary text and the critical path.
    Ink,
    /// Secondary text and ordinary edges.
    Graphite,
    /// Borders.
    Rule,
    /// Hover and sunken areas.
    Wash,
    /// Selection and focus.
    Accent,
    /// A light tint of the accent.
    AccentWash,
    /// A rating color.
    Tone(Tone),
}

/// A line style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// Color token.
    pub paint: Paint,
    /// Width in screen pixels.
    pub width: f32,
    /// Dashed instead of solid (shared-subtree links).
    pub dashed: bool,
}

impl Stroke {
    /// A solid line.
    #[must_use]
    pub const fn solid(paint: Paint, width: f32) -> Self {
        Stroke {
            paint,
            width,
            dashed: false,
        }
    }

    /// A dashed line.
    #[must_use]
    pub const fn dashed(paint: Paint, width: f32) -> Self {
        Stroke {
            paint,
            width,
            dashed: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_spread_over_the_buckets_and_the_extremes_are_fixed() {
        for count in 2..=12 {
            assert_eq!(Tone::from_level(0, count), Tone::VeryLow, "count {count}");
            assert_eq!(
                Tone::from_level(count - 1, count),
                Tone::High,
                "count {count}"
            );
            let mut last = 0;
            for i in 0..count {
                let bucket = match Tone::from_level(i, count) {
                    Tone::VeryLow => 0,
                    Tone::Low => 1,
                    Tone::Medium => 2,
                    Tone::High => 3,
                    Tone::Unrated => unreachable!(),
                };
                assert!(bucket >= last, "monotonic at {i}/{count}");
                last = bucket;
            }
        }
        assert_eq!(Tone::from_level(99, 3), Tone::High, "out of range clamps");
    }
}
