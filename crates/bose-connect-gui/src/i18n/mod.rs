//! The GUI's one translation layer: [Fluent] messages, one file per
//! language under `locales/`, embedded in the binary by `build.rs`.
//!
//! The language follows the desktop (`LANGUAGE`, `LC_ALL`,
//! `LC_MESSAGES`, `LANG`, in GLib's order); English (`en.ftl`, the
//! source language) fills in for every message a locale lacks. Use
//! [`t!`](crate::t) for every user-visible string:
//!
//! ```ignore
//! label.set_text(&t!("hero-none"));
//! label.set_text(&t!("hero-connecting", name = device_name));
//! ```
//!
//! Adding a language is one new `locales/<language>.ftl`; see
//! `CONTRIBUTING.md`, "Translating the GUI".
//!
//! [Fluent]: https://projectfluent.org/

use std::sync::OnceLock;

use bose_connect::{BdAddr, PromptLanguage};
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

/// The source language, and the fallback for missing messages.
pub const SOURCE_LANGUAGE: &str = "en";

/// Translate message `id`, with optional `$name = value` arguments.
#[macro_export]
macro_rules! t {
    ($id:literal) => {
        $crate::i18n::tr($id, &[])
    };
    ($id:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::tr(
            $id,
            &[$((stringify!($name), $crate::i18n::FluentValue::from($value))),+],
        )
    };
}

#[doc(hidden)]
pub use fluent_bundle::FluentValue;

/// The bundled language tags, sorted (`["en", "es", …]`).
pub fn available_languages() -> Vec<&'static str> {
    LOCALES.iter().map(|(lang, _)| *lang).collect()
}

/// Pick the bundled languages for the desktop's `requested` ones
/// (GLib's `language_names()`: `es_MX.UTF-8`, `es_MX`, `es`, `C`),
/// best first and always ending with [`SOURCE_LANGUAGE`].
pub fn negotiate(requested: &[String], available: &[&'static str]) -> Vec<&'static str> {
    let mut chosen: Vec<&'static str> = Vec::new();
    for name in requested {
        // `es_MX.UTF-8@euro` -> `es-MX`; `C` and `POSIX` mean "none".
        let tag = name
            .split(['.', '@'])
            .next()
            .unwrap_or_default()
            .replace('_', "-");
        if tag.is_empty() || tag == "C" || tag == "POSIX" {
            continue;
        }
        let language = tag.split('-').next().unwrap_or_default();
        let found = available
            .iter()
            .find(|a| a.eq_ignore_ascii_case(&tag))
            .or_else(|| available.iter().find(|a| a.eq_ignore_ascii_case(language)));
        if let Some(lang) = found {
            if !chosen.contains(lang) {
                chosen.push(lang);
            }
        }
    }
    if !chosen.contains(&SOURCE_LANGUAGE) {
        chosen.push(SOURCE_LANGUAGE);
    }
    chosen
}

/// Parse one bundled locale. Panics on a syntax error: the tests
/// parse every file, so a broken one never ships.
fn bundle(lang: &str) -> FluentBundle<FluentResource> {
    let source = LOCALES
        .iter()
        .find(|(l, _)| *l == lang)
        .map(|(_, s)| *s)
        .unwrap_or_else(|| panic!("no locales/{lang}.ftl"));
    let resource = FluentResource::try_new(source.to_owned())
        .unwrap_or_else(|(_, errors)| panic!("locales/{lang}.ftl: {errors:?}"));
    let id: LanguageIdentifier = lang.parse().unwrap_or_default();
    let mut bundle = FluentBundle::new_concurrent(vec![id]);
    // No Unicode isolation marks around arguments: they show as
    // stray characters in GTK labels for left-to-right languages.
    bundle.set_use_isolating(false);
    bundle
        .add_resource(resource)
        .unwrap_or_else(|errors| panic!("locales/{lang}.ftl: {errors:?}"));
    bundle
}

/// The bundles for the desktop language, best first.
fn bundles() -> &'static [FluentBundle<FluentResource>] {
    static BUNDLES: OnceLock<Vec<FluentBundle<FluentResource>>> = OnceLock::new();
    BUNDLES.get_or_init(|| {
        let requested: Vec<String> = gtk::glib::language_names()
            .iter()
            .map(|n| n.to_string())
            .collect();
        let chosen = negotiate(&requested, &available_languages());
        tracing::debug!(target: "i18n", "languages: {chosen:?} (desktop: {requested:?})");
        chosen.into_iter().map(bundle).collect()
    })
}

/// Format `id` from the first bundle that has it.
fn format(
    bundles: &[FluentBundle<FluentResource>],
    id: &str,
    args: &[(&str, FluentValue<'_>)],
) -> Option<String> {
    let mut fluent_args = FluentArgs::new();
    for (name, value) in args {
        fluent_args.set(*name, value.clone());
    }
    bundles.iter().find_map(|bundle| {
        let pattern = bundle.get_message(id)?.value()?;
        let mut errors = Vec::new();
        let text = bundle.format_pattern(pattern, Some(&fluent_args), &mut errors);
        if !errors.is_empty() {
            tracing::warn!(target: "i18n", "{id}: {errors:?}");
        }
        Some(text.into_owned())
    })
}

/// Translate message `id` in the desktop language. A missing message
/// shows its id, so it is easy to spot; prefer the [`t!`](crate::t)
/// macro.
pub fn tr(id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
    format(bundles(), id, args).unwrap_or_else(|| {
        tracing::warn!(target: "i18n", "missing message {id}");
        id.to_owned()
    })
}

/// Display label for a [`PromptLanguage`].
pub fn language_label(lang: PromptLanguage) -> &'static str {
    match lang {
        PromptLanguage::En => "English",
        PromptLanguage::Fr => "French",
        PromptLanguage::It => "Italian",
        PromptLanguage::De => "German",
        PromptLanguage::Es => "Spanish",
        PromptLanguage::Pt => "Portuguese",
        PromptLanguage::Zh => "Chinese",
        PromptLanguage::Ko => "Korean",
        PromptLanguage::Ru => "Russian",
        PromptLanguage::Pl => "Polish",
        PromptLanguage::Nl => "Dutch",
        PromptLanguage::Ja => "Japanese",
        PromptLanguage::Sv => "Swedish",
    }
}

/// Display string for a Bluetooth address in canonical form.
pub fn format_address(addr: BdAddr) -> String {
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        addr.b[0], addr.b[1], addr.b[2], addr.b[3], addr.b[4], addr.b[5]
    )
}

/// Lookup table consulted by `crate::transport::prompt_language_label`.
pub(crate) const LANG_NAMES: &[(u8, &str)] = &[
    (0x21, "English"),
    (0x22, "Français"),
    (0x23, "Italiano"),
    (0x24, "Deutsch"),
    (0x26, "Español"),
    (0x27, "Português"),
    (0x28, "中文"),
    (0x29, "한국어"),
    (0x2a, "Русский"),
    (0x2b, "Polski"),
    (0x2e, "Nederlands"),
    (0x2f, "日本語"),
    (0x32, "Svenska"),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The message ids of one locale file.
    fn ids(lang: &str) -> Vec<String> {
        let source = LOCALES.iter().find(|(l, _)| *l == lang).unwrap().1;
        let resource = FluentResource::try_new(source.to_owned()).expect("parses");
        let mut ids: Vec<String> = resource
            .entries()
            .filter_map(|entry| match entry {
                fluent_syntax::ast::Entry::Message(m) => Some(m.id.name.to_owned()),
                _ => None,
            })
            .collect();
        ids.sort();
        ids
    }

    #[test]
    fn every_locale_parses_and_has_the_english_messages() {
        let english = ids(SOURCE_LANGUAGE);
        for lang in available_languages() {
            drop(bundle(lang));
            assert_eq!(ids(lang), english, "locales/{lang}.ftl vs en.ftl");
        }
    }

    #[test]
    fn spanish_and_english_are_bundled() {
        assert!(available_languages().contains(&"en"));
        assert!(available_languages().contains(&"es"));
    }

    #[test]
    fn every_message_the_code_uses_is_in_english() {
        let english = ids(SOURCE_LANGUAGE);
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut missing = Vec::new();
        let mut stack = vec![src];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                // The `t!` macro, but not the tail of `format!`.
                let marker = "t!(\"";
                for (at, _) in text.match_indices(marker) {
                    let before = text[..at].chars().next_back();
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    let id = text[at + marker.len()..].split('"').next().unwrap();
                    if !english.iter().any(|e| e == id) {
                        missing.push(format!("{}: {id}", path.display()));
                    }
                }
            }
        }
        assert!(missing.is_empty(), "not in en.ftl: {missing:?}");
    }

    #[test]
    fn negotiation_follows_the_desktop_and_ends_in_english() {
        let langs = ["en", "es"];
        let req = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(
            negotiate(&req(&["es_MX.UTF-8", "es_MX", "es", "C"]), &langs),
            ["es", "en"]
        );
        assert_eq!(negotiate(&req(&["de_DE", "de", "C"]), &langs), ["en"]);
        assert_eq!(negotiate(&req(&["C"]), &langs), ["en"]);
        assert_eq!(negotiate(&req(&["en_GB", "es"]), &langs), ["en", "es"]);
    }

    #[test]
    fn arguments_and_fallback() {
        let chain = vec![bundle("es"), bundle("en")];
        assert_eq!(
            format(
                &chain,
                "hero-connecting",
                &[("name", FluentValue::from("QC"))]
            )
            .unwrap(),
            "Conectando a QC…"
        );
        assert_eq!(
            format(
                &chain[1..],
                "volume-value",
                &[
                    ("level", FluentValue::from(45)),
                    ("percent", FluentValue::from(60)),
                ]
            )
            .unwrap(),
            "45 / 75 (60%)"
        );
        assert_eq!(format(&chain, "no-such-message", &[]), None);
    }
}
