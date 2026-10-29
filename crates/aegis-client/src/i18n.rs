//! Translation for text that the client core builds itself: pages, errors
//! and reports (L10N-01). The Slint window translates its own `@tr()` text
//! from bundled gettext catalogs (see `crates/aegis-shell`); this module is
//! the Fluent side, one catalog per language, bundled in the binary.
//!
//! Catalogs live in `locales/{en,ja,pseudo}/main.ftl`. The pseudo-locale
//! transforms the English source so clipped or hard-coded text shows up in
//! screenshots (L10N-03); it is not shipped to users.
//!
//! A [`Catalog`] is created on demand and is not `Send`, because it is only
//! used while a report or a page is being built (on the blocking pool, or on
//! the session thread). [`Language`] is the cheap, `Copy` choice that the
//! view model and settings carry.

use std::fmt;

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

const EN: &str = include_str!("../locales/en/main.ftl");
const JA: &str = include_str!("../locales/ja/main.ftl");
const PSEUDO: &str = include_str!("../locales/pseudo/main.ftl");

/// The languages a report or the UI can be written in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Language {
    /// English.
    #[default]
    En,
    /// Japanese.
    Ja,
    /// The pseudo-locale, for testing clipping (not shipped to users).
    Pseudo,
}

impl Language {
    /// The language tag used for the Fluent bundle and the PDF metadata.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Ja => "ja",
            Language::Pseudo => "en",
        }
    }

    /// The name in its own language, for the setting UI.
    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Language::En => "English",
            Language::Ja => "日本語",
            Language::Pseudo => "Pseudo",
        }
    }

    /// Every language a shipped app offers (the pseudo-locale is hidden
    /// behind it only for tests).
    #[must_use]
    pub fn all() -> [Language; 3] {
        [Language::En, Language::Ja, Language::Pseudo]
    }

    /// The language from a code like `"en"`, `"ja"` or `"pseudo"`.
    #[must_use]
    pub fn from_code(code: &str) -> Language {
        match code {
            "ja" => Language::Ja,
            "pseudo" => Language::Pseudo,
            _ => Language::En,
        }
    }

    /// The language the OS reports, or English. `AEGIS_LANGUAGE` overrides it
    /// for tests and air-gapped deployments.
    #[must_use]
    pub fn from_os() -> Language {
        if let Some(v) = std::env::var_os("AEGIS_LANGUAGE")
            && let Some(s) = v.to_str()
        {
            return Language::from_code(s);
        }
        let hint = [
            std::env::var("LC_ALL").ok(),
            std::env::var("LC_MESSAGES").ok(),
            std::env::var("LANG").ok(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        if hint.to_lowercase().contains("ja") {
            Language::Ja
        } else {
            Language::En
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Drops the invisible Unicode bidi-control characters that Fluent inserts
/// around interpolated values (U+2068/U+2069 and friends).
fn strip_bidi_controls(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                *c,
                '\u{200E}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
            )
        })
        .collect()
}

/// A loaded Fluent catalog for one language.
pub struct Catalog {
    bundle: FluentBundle<FluentResource>,
}

impl Catalog {
    /// Loads the bundled catalog for `language`.
    #[must_use]
    pub fn load(language: Language) -> Catalog {
        let text = match language {
            Language::En => EN,
            Language::Ja => JA,
            Language::Pseudo => PSEUDO,
        };
        let lang: LanguageIdentifier = language
            .code()
            .parse()
            .unwrap_or_else(|_| "en".parse().expect("the fallback language tag parses"));
        let mut bundle = FluentBundle::new(vec![lang]);
        let resource =
            FluentResource::try_new(text.to_owned()).expect("the bundled Fluent catalog parses");
        bundle
            .add_resource(resource)
            .expect("the bundled Fluent catalog loads");
        Catalog { bundle }
    }

    /// Looks up a message with no arguments. Missing keys return the key in
    /// brackets, so a gap is visible instead of a silent empty string.
    #[must_use]
    pub fn tr(&self, id: &str) -> String {
        self.tr_with(id, &[])
    }

    /// Looks up a message and fills its placeholders. Each argument is a
    /// `(name, value)` pair; values are strings (numbers are formatted by
    /// the caller so a locale can choose its own separators).
    ///
    /// Fluent wraps interpolated values in Unicode bidi-isolation marks
    /// (U+2068/U+2069); they are stripped here so reports, captions and file
    /// names match what the reader sees (they are invisible but would break
    /// byte-for-byte comparisons).
    #[must_use]
    pub fn tr_with(&self, id: &str, args: &[(&str, &str)]) -> String {
        let Some(message) = self.bundle.get_message(id) else {
            return format!("[{id}]");
        };
        let Some(pattern) = message.value() else {
            return format!("[{id}]");
        };
        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            fluent_args.set(*name, *value);
        }
        let mut errors = Vec::new();
        let value = self
            .bundle
            .format_pattern(pattern, Some(&fluent_args), &mut errors)
            .into_owned();
        let value = strip_bidi_controls(&value);
        if value.is_empty() {
            format!("[{id}]")
        } else {
            value
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_loads_and_answers_a_key() {
        for lang in Language::all() {
            let c = Catalog::load(lang);
            let title = c.tr_with("report-title", &[("project", "Brake ECU")]);
            assert!(
                !title.is_empty() && title.contains("Brake ECU"),
                "{lang}: {title}"
            );
        }
    }

    #[test]
    fn a_missing_key_is_visible_not_silent() {
        let c = Catalog::load(Language::En);
        assert_eq!(c.tr("no-such-message"), "[no-such-message]");
    }

    #[test]
    fn the_os_language_falls_back_to_english() {
        assert_eq!(Language::from_code("ja"), Language::Ja);
        assert_eq!(Language::from_code("de"), Language::En);
        assert_eq!(Language::from_code(""), Language::En);
    }
}
