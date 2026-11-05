//! Interface translations. See `docs/localization.md`.

use crate::platform::preferred_ui_languages;
use i18n_embed::LanguageLoader;
use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::unic_langid::LanguageIdentifier;
use rust_embed::RustEmbed;
use std::sync::LazyLock;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

/// Starts in English until [`init`] selects the Windows display language.
pub static LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(english_loader);

/// Always English, for logs, so bug reports stay readable.
pub static ENGLISH: LazyLock<FluentLanguageLoader> = LazyLock::new(english_loader);

fn english_loader() -> FluentLanguageLoader {
    let loader = fluent_language_loader!();
    apply_languages(&loader, &[]);
    loader
}

/// Returns a translated string. Message IDs and argument names are checked at compile time against
/// the English file.
macro_rules! t {
    ($($args:tt)*) => {
        i18n_embed_fl::fl!($crate::i18n::LOADER, $($args)*)
    };
}
pub(crate) use t;

/// Like [`t!`], but always returns English. Use it for text that goes to the logs.
macro_rules! t_en {
    ($($args:tt)*) => {
        i18n_embed_fl::fl!($crate::i18n::ENGLISH, $($args)*)
    };
}
pub(crate) use t_en;

pub fn init() {
    let languages = preferred_ui_languages();
    let requested: Vec<LanguageIdentifier> = languages
        .iter()
        .filter_map(|tag| tag.parse().ok())
        .collect();
    apply_languages(&LOADER, &requested);
    log::info!(
        "Windows languages: {languages:?}, using: {}",
        LOADER.current_language()
    );
}

fn apply_languages(loader: &FluentLanguageLoader, requested: &[LanguageIdentifier]) {
    if let Err(e) = i18n_embed::select(loader, &Localizations, requested) {
        log::error!("Failed to load translations: {e:#}");
    }
    // Isolation marks are invisible characters around each argument, meant for mixed-direction text
    // in rich UIs. Keep them out of tray menu labels and notifications.
    loader.set_use_isolating(false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
    use std::collections::{BTreeMap, BTreeSet, HashMap};

    fn collect_inline<'a>(expression: &InlineExpression<&'a str>, out: &mut BTreeSet<&'a str>) {
        match expression {
            InlineExpression::VariableReference { id } => {
                out.insert(id.name);
            }
            InlineExpression::Placeable { expression } => collect_expression(expression, out),
            InlineExpression::FunctionReference { arguments, .. } => {
                for argument in &arguments.positional {
                    collect_inline(argument, out);
                }
                for argument in &arguments.named {
                    collect_inline(&argument.value, out);
                }
            }
            _ => {}
        }
    }

    fn collect_expression<'a>(expression: &Expression<&'a str>, out: &mut BTreeSet<&'a str>) {
        match expression {
            Expression::Inline(inline) => collect_inline(inline, out),
            Expression::Select { selector, variants } => {
                collect_inline(selector, out);
                for variant in variants {
                    collect_pattern(&variant.value, out);
                }
            }
        }
    }

    fn collect_pattern<'a>(pattern: &Pattern<&'a str>, out: &mut BTreeSet<&'a str>) {
        for element in &pattern.elements {
            if let PatternElement::Placeable { expression } = element {
                collect_expression(expression, out);
            }
        }
    }

    /// Maps each message ID in a Fluent file to the arguments it uses.
    fn messages(source: &str) -> BTreeMap<&str, BTreeSet<&str>> {
        let resource = fluent_syntax::parser::parse(source)
            .unwrap_or_else(|(_, errors)| panic!("invalid Fluent file: {errors:?}"));
        resource
            .body
            .iter()
            .filter_map(|entry| match entry {
                Entry::Message(message) => {
                    let mut arguments = BTreeSet::new();
                    if let Some(pattern) = &message.value {
                        collect_pattern(pattern, &mut arguments);
                    }
                    Some((message.id.name, arguments))
                }
                _ => None,
            })
            .collect()
    }

    /// Missing messages are allowed because they fall back to English.
    fn translation_problems(english: &str, translation: &str) -> Vec<String> {
        let english = messages(english);
        let mut problems = Vec::new();
        for (id, arguments) in messages(translation) {
            match english.get(id) {
                None => problems.push(format!("`{id}` does not exist in English")),
                Some(known) => problems.extend(
                    arguments
                        .difference(known)
                        .map(|argument| format!("`{id}` uses unknown argument `{argument}`")),
                ),
            }
        }
        problems
    }

    fn embedded(path: &str) -> String {
        let file = Localizations::get(path).expect("embedded file should exist");
        String::from_utf8(file.data.into_owned()).expect("Fluent files should be UTF-8")
    }

    #[test]
    fn translations_match_english() {
        let english_path = "en/volume_locker.ftl";
        let english = embedded(english_path);
        assert!(!messages(&english).is_empty());
        for path in Localizations::iter().filter(|path| path != english_path) {
            // The loader only finds `<language tag>/volume_locker.ftl`, so anything else would
            // silently never be shown.
            let language = path
                .strip_suffix("/volume_locker.ftl")
                .unwrap_or_else(|| panic!("{path}: expected <language tag>/volume_locker.ftl"));
            language
                .parse::<LanguageIdentifier>()
                .unwrap_or_else(|e| panic!("{path}: invalid language tag: {e}"));
            let problems = translation_problems(&english, &embedded(&path));
            assert!(problems.is_empty(), "{path}: {problems:?}");
        }
    }

    #[test]
    fn translation_check_finds_problems() {
        let english = "known = Hello { $name }\nother = Other\n";
        assert!(translation_problems(english, "known = Olá { $name }\n").is_empty());
        assert_eq!(
            translation_problems(english, "stale = Old\nknown = Olá { $nome }\n"),
            [
                "`known` uses unknown argument `nome`",
                "`stale` does not exist in English",
            ]
        );
        assert_eq!(
            translation_problems(english, "known = { NUMBER($nome) }\n"),
            ["`known` uses unknown argument `nome`"]
        );
    }

    #[test]
    fn unavailable_language_falls_back_to_english_without_isolation_marks() {
        let loader = fluent_language_loader!();
        let requested = ["pt-BR".parse().expect("valid language tag")];
        apply_languages(&loader, &requested);

        assert_eq!(loader.current_language().to_string(), "en");
        assert_eq!(
            loader.get_args("current-device", HashMap::from([("device", "Speakers")])),
            "Current: Speakers"
        );
    }

    #[test]
    fn macro_formats_full_sentences() {
        assert_eq!(
            t!(
                "volume-restored",
                device = "Speakers",
                from = "50",
                to = "100"
            ),
            "The volume of Speakers has been restored from 50% to 100%."
        );
    }

    #[test]
    fn english_macro_formats_full_sentences() {
        assert_eq!(
            t_en!("current-device", device = "Speakers"),
            "Current: Speakers"
        );
    }

    #[test]
    fn windows_reports_valid_languages() {
        let languages = preferred_ui_languages();
        assert!(!languages.is_empty());
        for tag in languages {
            tag.parse::<LanguageIdentifier>()
                .expect("Windows should report valid language tags");
        }
    }
}
