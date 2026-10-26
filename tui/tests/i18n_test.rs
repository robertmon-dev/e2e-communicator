use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use tui::i18n::{
    EN, Locale, Locale as L, MISSING_CLOSE, MISSING_OPEN, Segment, TOKEN_CLOSE, TOKEN_OPEN,
    segments, segments_of, t, template, tf,
};

fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn source_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                found.push(path);
            }
        }
    }

    let mut found = Vec::new();
    walk(&source_root(), &mut found);
    found.sort();
    assert!(found.len() > 5, "the source scan found almost nothing");
    found
}

fn sources_without_the_catalogue() -> Vec<(PathBuf, String)> {
    source_files()
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name != "i18n.rs"))
        .map(|path| {
            let text = std::fs::read_to_string(&path).unwrap();
            (path, text)
        })
        .collect()
}

fn namespaces() -> BTreeSet<&'static str> {
    EN.iter()
        .map(|(key, _)| key.split('.').next().unwrap())
        .collect()
}

fn dotted_literals(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes: Vec<char> = text.chars().collect();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != '"' {
            index += 1;
            continue;
        }

        let mut literal = String::new();
        index += 1;
        while index < bytes.len() && bytes[index] != '"' {
            if bytes[index] == '\\' {
                index += 2;
                continue;
            }
            literal.push(bytes[index]);
            index += 1;
        }
        index += 1;

        let dotted = literal.contains('.')
            && literal
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '.');
        if dotted {
            found.push(literal);
        }
    }

    found
}

#[test]
fn the_catalogue_is_sorted_and_free_of_duplicates() {
    let keys: Vec<&str> = EN.iter().map(|(key, _)| *key).collect();

    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted, "the catalogue must stay sorted by key");

    let unique: BTreeSet<&str> = keys.iter().copied().collect();
    assert_eq!(unique.len(), keys.len(), "a key is listed twice");
}

#[test]
fn every_entry_has_text() {
    for (key, text) in EN {
        assert!(!text.trim().is_empty(), "{key} has no text");
    }
}

#[test]
fn every_key_used_in_the_sources_exists() {
    let namespaces = namespaces();
    let mut missing = Vec::new();

    for (path, text) in sources_without_the_catalogue() {
        for literal in dotted_literals(&text) {
            let namespace = literal.split('.').next().unwrap();
            if !namespaces.contains(namespace) {
                continue;
            }
            if template(&literal).is_none() {
                missing.push(format!("{}: {literal}", path.display()));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "these look like catalogue keys but are not in it:\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_catalogue_key_is_used() {
    let sources: String = sources_without_the_catalogue()
        .into_iter()
        .map(|(_, text)| text)
        .collect();

    let unused: Vec<&str> = EN
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| !sources.contains(&format!("\"{key}\"")))
        .collect();

    assert!(
        unused.is_empty(),
        "these keys are in the catalogue but nothing uses them: {unused:?}"
    );
}

#[test]
fn the_sources_carry_no_polish_text() {
    let polish = "ąćęłńóśźżĄĆĘŁŃÓŚŹŻ";
    let mut offenders = Vec::new();

    for (path, text) in sources_without_the_catalogue() {
        for (number, line) in text.lines().enumerate() {
            if line.chars().any(|c| polish.contains(c)) {
                offenders.push(format!("{}:{}: {line}", path.display(), number + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "Polish text outside the catalogue:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn a_lookup_returns_the_text_and_a_miss_is_visible() {
    assert_eq!(t("command.cancelled"), "cancelled");

    let miss = t("nope.not_here");
    assert!(
        miss.starts_with(MISSING_OPEN) && miss.ends_with(MISSING_CLOSE),
        "a missing key must be obvious on screen, got {miss}"
    );
    assert!(miss.contains("nope.not_here"), "got {miss}");
}

#[test]
fn placeholders_are_substituted_and_leftovers_stay_visible() {
    assert_eq!(
        tf("notify.connectivity", &[("status", "Online")]),
        "connectivity: Online"
    );

    assert_eq!(
        tf(
            "notify.connectivity",
            &[("status", "Online"), ("unused", "x")]
        ),
        "connectivity: Online"
    );

    assert_eq!(tf("notify.connectivity", &[]), "connectivity: {status}");
}

#[test]
fn a_template_with_several_placeholders_fills_all_of_them() {
    assert_eq!(
        tf(
            "error.line",
            &[
                ("code", "-32005"),
                ("explanation", "wrong state"),
                ("message", "no identity"),
            ]
        ),
        "error -32005: wrong state (no identity)"
    );
}

#[test]
fn locale_tags_are_parsed_the_way_the_environment_spells_them() {
    for tag in ["en", "EN", "en_GB", "en_US.UTF-8", "en-US", "C", "POSIX"] {
        assert_eq!(Locale::parse(tag), Some(Locale::En), "tag {tag}");
    }

    for tag in ["pl", "pl_PL.UTF-8", "de", "", "nonsense"] {
        assert_eq!(Locale::parse(tag), None, "tag {tag}");
    }
}

#[test]
fn every_locale_has_the_same_keys_as_english() {
    #[allow(clippy::single_element_loop)]
    for locale in [L::En] {
        let keys: BTreeSet<&str> = locale.catalogue().iter().map(|(key, _)| *key).collect();
        let english: BTreeSet<&str> = EN.iter().map(|(key, _)| *key).collect();

        let missing: Vec<&&str> = english.difference(&keys).collect();
        let extra: Vec<&&str> = keys.difference(&english).collect();

        assert!(missing.is_empty(), "{locale:?} is missing {missing:?}");
        assert!(extra.is_empty(), "{locale:?} has unknown keys {extra:?}");
    }
}

#[test]
fn every_locale_keeps_the_placeholders_english_declares() {
    fn placeholders(template: &str) -> BTreeSet<String> {
        let mut found = BTreeSet::new();
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            let Some(close) = rest[open..].find('}') else {
                break;
            };
            found.insert(rest[open + 1..open + close].to_string());
            rest = &rest[open + close..];
        }
        found
    }

    #[allow(clippy::single_element_loop)]
    for locale in [L::En] {
        for (key, text) in locale.catalogue() {
            let english = EN
                .iter()
                .find(|(candidate, _)| candidate == key)
                .map(|(_, text)| *text)
                .unwrap();

            assert_eq!(
                placeholders(text),
                placeholders(english),
                "{locale:?} key {key} does not take the same placeholders as English"
            );
        }
    }
}

#[test]
fn markup_splits_into_plain_runs_and_tokens() {
    assert_eq!(
        segments("press [[alt]] + [[h]] to dock"),
        vec![
            Segment::Plain("press ".to_string()),
            Segment::Token("alt".to_string()),
            Segment::Plain(" + ".to_string()),
            Segment::Token("h".to_string()),
            Segment::Plain(" to dock".to_string()),
        ]
    );

    assert_eq!(
        segments("nothing marked"),
        vec![Segment::Plain("nothing marked".to_string())]
    );
}

#[test]
fn an_unterminated_marker_stays_plain_text() {
    assert_eq!(
        segments("press [[alt to dock"),
        vec![Segment::Plain("press [[alt to dock".to_string())]
    );
}

#[test]
fn markup_survives_placeholder_substitution() {
    let parts = segments_of("status.no_client", &[("reason", "gone")]);
    let joined: String = parts
        .iter()
        .map(|part| match part {
            Segment::Plain(text) | Segment::Token(text) => text.as_str(),
        })
        .collect();

    assert_eq!(joined, "no client (gone)");
}

#[test]
fn plain_lookups_never_leak_the_markers() {
    for (key, _) in EN {
        let plain = t(key);
        assert!(
            !plain.contains(TOKEN_OPEN) && !plain.contains(TOKEN_CLOSE),
            "{key} renders markers as text: {plain}"
        );
    }

    assert_eq!(
        t("help.keys_focus"),
        "alt + arrows move focus between the panes",
        "the markers are stripped, not rendered"
    );
}

#[test]
fn every_marker_in_the_catalogue_is_closed() {
    for (key, template) in EN {
        let opens = template.matches(TOKEN_OPEN).count();
        let closes = template.matches(TOKEN_CLOSE).count();
        assert_eq!(opens, closes, "{key} has unbalanced markers: {template}");

        for part in segments(template) {
            if let Segment::Token(text) = part {
                assert!(!text.is_empty(), "{key} has an empty token");
                assert!(
                    !text.contains('\n'),
                    "{key} marks up a line break, which cannot carry a style"
                );
            }
        }
    }
}
