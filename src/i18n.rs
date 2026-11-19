//! Translate application-owned templates before inserting any profile or site data.
use crate::storage::Language;
use std::{collections::HashMap, sync::OnceLock};

pub fn template(source: &str, language: Language) -> String {
    if language == Language::Ru {
        return source.into();
    }
    static TRANSLATIONS: OnceLock<HashMap<char, Vec<(&str, &str)>>> = OnceLock::new();
    let translations = TRANSLATIONS.get_or_init(|| {
        let mut translations = HashMap::<char, Vec<(&str, &str)>>::new();
        for (russian, english) in include_str!("../ui/strings.tsv")
            .lines()
            .filter_map(|line| line.split_once('\t'))
        {
            if let Some(first) = russian.chars().next() {
                translations
                    .entry(first)
                    .or_default()
                    .push((russian, english));
            }
        }
        for entries in translations.values_mut() {
            entries.sort_unstable_by_key(|(russian, _)| std::cmp::Reverse(russian.len()));
        }
        translations
    });
    fn is_cyrillic(c: char) -> bool {
        ('\u{0400}'..='\u{04ff}').contains(&c)
    }

    let mut text = String::with_capacity(source.len());
    let mut rest = source;
    let mut previous = None;
    while let Some(current) = rest.chars().next() {
        if !previous.is_some_and(is_cyrillic) {
            if let Some((english, suffix)) = translations.get(&current).and_then(|entries| {
                entries.iter().find_map(|(russian, english)| {
                    let suffix = rest.strip_prefix(russian)?;
                    (!suffix.starts_with(is_cyrillic)).then_some((english, suffix))
                })
            }) {
                text.push_str(english);
                rest = suffix;
                previous = None;
                continue;
            }
        }
        text.push(current);
        rest = &rest[current.len_utf8()..];
        previous = Some(current);
    }
    text.replace("lang=\"ru\"", "lang=\"en\"")
}
