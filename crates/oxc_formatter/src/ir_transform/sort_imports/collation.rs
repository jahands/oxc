use std::{borrow::Cow, cmp::Ordering, iter::Peekable, str::CharIndices};

use cow_utils::CowUtils;

use super::options::{SortCollation, SortImportsOptions};

/// ASCII symbols and whitespace in CLDR root collation order, lowest first.
const SYMBOL_ORDER: &[u8] = b"\t\n\x0b\x0c\r _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

/// Returns the sort key of an import source or specifier name.
pub fn normalize_name<'a>(name: &'a str, options: &SortImportsOptions) -> Cow<'a, str> {
    if options.ignore_case && options.collation == SortCollation::Natural {
        name.cow_to_lowercase()
    } else {
        Cow::Borrowed(name)
    }
}

/// Compares two keys returned by [`normalize_name`] in ascending order.
pub fn compare_names(a: &str, b: &str, options: &SortImportsOptions) -> Ordering {
    match options.collation {
        SortCollation::Natural => natord::compare(a, b),
        SortCollation::Intl if options.ignore_case => intl_compare(a, b),
        SortCollation::Intl => intl_compare(a, b).then_with(|| natord::compare(a, b)),
    }
}

/// Compares two strings like
/// `new Intl.Collator('en', { sensitivity: 'base', numeric: true }).compare(a, b)`.
///
/// ASCII input matches ICU's root collation exactly:
/// - control characters are ignored
/// - symbols and whitespace sort before digits, in the order of [`SYMBOL_ORDER`]
/// - a run of digits is one element ordered by its numeric value (leading zeros are ignored)
/// - letters sort after digits, case-insensitively
///
/// A non-ASCII character sorts after all ASCII letters, ordered by its lowercase code point.
/// ICU orders those by script and folds accents, so non-ASCII input can differ from ICU.
///
/// Strings that differ only in case or ignored characters compare as `Equal`.
pub fn intl_compare(a: &str, b: &str) -> Ordering {
    let mut a = CollationElements::new(a);
    let mut b = CollationElements::new(b);
    loop {
        match (a.next(), b.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match x.cmp(&y) {
                Ordering::Equal => {}
                ordering => return ordering,
            },
        }
    }
}

/// The primary weight of one collation element.
/// Variant order is the weight order across kinds.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Element<'a> {
    /// Index into [`SYMBOL_ORDER`].
    Symbol(usize),
    /// A digit run without leading zeros (`"0"` for an all-zero run).
    /// Comparing the length first orders runs by numeric value.
    Number { len: usize, digits: &'a str },
    /// Lowercase ASCII letter.
    Letter(u8),
    /// Lowercase non-ASCII character.
    Other(char),
}

struct CollationElements<'a> {
    text: &'a str,
    chars: Peekable<CharIndices<'a>>,
}

impl<'a> CollationElements<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, chars: text.char_indices().peekable() }
    }
}

impl<'a> Iterator for CollationElements<'a> {
    type Item = Element<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (start, c) = self.chars.next()?;

            if c.is_ascii_digit() {
                let mut end = start + 1;
                while let Some(&(idx, next)) = self.chars.peek()
                    && next.is_ascii_digit()
                {
                    end = idx + 1;
                    self.chars.next();
                }
                let run = &self.text[start..end];
                let trimmed = run.trim_start_matches('0');
                let digits = if trimmed.is_empty() { &run[run.len() - 1..] } else { trimmed };
                return Some(Element::Number { len: digits.len(), digits });
            }

            if c.is_ascii_alphabetic() {
                return Some(Element::Letter(c.to_ascii_lowercase() as u8));
            }

            if c.is_ascii() {
                // Characters missing from the table are control characters, which ICU ignores.
                if let Some(rank) = SYMBOL_ORDER.iter().position(|&s| s == c as u8) {
                    return Some(Element::Symbol(rank));
                }
                continue;
            }

            return Some(Element::Other(c.to_lowercase().next().unwrap_or(c)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sorted with `Intl.Collator('en', { sensitivity: 'base', numeric: true })` in Node 22 (ICU 78).
    /// Adjacent entries joined by `=` compare equal.
    const INTL_SORTED: &[&str] = &[
        "",
        "\t",
        " ",
        "_",
        "_a",
        "-",
        "-a",
        ",",
        ";",
        ":",
        "!",
        "?",
        ".",
        "../",
        "./",
        "./a",
        "'",
        "\"",
        "(",
        ")",
        "[",
        "]",
        "{",
        "}",
        "@",
        "@/a",
        "@grafana/x",
        "*",
        "/",
        "\\",
        "&",
        "#",
        "#lib/a",
        "%",
        "`",
        "^",
        "+",
        "<",
        "=",
        ">",
        "|",
        "~",
        "~/a",
        "$",
        "$a",
        "0",
        "1",
        "01",
        "2",
        "10",
        "a",
        "A",
        "a_b",
        "a-b",
        "a:b",
        "a/b",
        "a$",
        "a0",
        "a00",
        "a1",
        "a01",
        "a1b",
        "a2",
        "a10",
        "ab",
        "AB",
        "b",
        "bun",
        "bun:test",
        "z",
    ];

    #[test]
    fn matches_intl_collator() {
        let equal_pairs = [("1", "01"), ("a", "A"), ("a0", "a00"), ("a1", "a01"), ("ab", "AB")];
        for pair in INTL_SORTED.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let expected =
                if equal_pairs.contains(&(a, b)) { Ordering::Equal } else { Ordering::Less };
            assert_eq!(intl_compare(a, b), expected, "{a:?} vs {b:?}");
            assert_eq!(intl_compare(b, a), expected.reverse(), "{b:?} vs {a:?}");
        }
    }

    #[test]
    fn ignores_control_characters() {
        assert_eq!(intl_compare("a\u{0}b", "ab"), Ordering::Equal);
        assert_eq!(intl_compare("a\u{7f}", "a"), Ordering::Equal);
    }
}
