//! Removal of characters XML 1.0 cannot represent at all.
//!
//! Escaping does not help here. `&`, `<` and `>` have entity forms; a `0x0B`
//! has none — it is outside the `Char` production, so a document containing one
//! is malformed however it is written, and XML parsers are draconian: no error
//! recovery, the whole feed is rejected. So these are dropped, not encoded.

/// Whether `character` is inside XML 1.0's `Char` production.
///
/// `#x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`.
/// Surrogates cannot appear in a Rust `str`, so only the control range and the
/// two noncharacters at the end of the BMP need testing.
const fn is_legal(character: char) -> bool {
    matches!(character, '\u{9}' | '\u{A}' | '\u{D}')
        || (character >= '\u{20}' && character <= '\u{D7FF}')
        || (character >= '\u{E000}' && character <= '\u{FFFD}')
        || character >= '\u{10000}'
}

/// Drops every character XML 1.0 cannot represent.
///
/// Returns the cleaned text and how many characters were removed, because a
/// control character in a blog post means something upstream is broken and the
/// caller reports that.
pub fn strip_illegal(text: &str) -> (String, usize) {
    if text.chars().all(is_legal) {
        return (String::from(text), 0);
    }

    let mut cleaned: String = String::with_capacity(text.len());
    let mut removed: usize = 0;
    for character in text.chars() {
        if is_legal(character) {
            cleaned.push(character);
        } else {
            removed += 1;
        }
    }
    (cleaned, removed)
}

#[cfg(test)]
mod tests {
    use super::strip_illegal;

    #[test]
    fn ordinary_text_is_returned_unchanged_and_reports_nothing_removed() {
        let expected: (String, usize) = (String::from("Hello, <world> & co."), 0);
        let actual: (String, usize) = strip_illegal("Hello, <world> & co.");
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_three_legal_control_characters_survive() {
        let expected: (String, usize) = (String::from("a\tb\nc\rd"), 0);
        let actual: (String, usize) = strip_illegal("a\tb\nc\rd");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_vertical_tab_is_dropped_because_no_escape_could_have_saved_it() {
        let expected: (String, usize) = (String::from("ab"), 1);
        let actual: (String, usize) = strip_illegal("a\u{0B}b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_illegal_control_character_is_counted() {
        let input: String = (0x00..0x20_u32)
            .filter(|code| !matches!(code, 0x09 | 0x0A | 0x0D))
            .filter_map(char::from_u32)
            .collect();

        let expected: (String, usize) = (String::new(), 29);
        let actual: (String, usize) = strip_illegal(&input);
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_two_noncharacters_at_the_end_of_the_bmp_are_dropped() {
        let expected: (String, usize) = (String::from("ab"), 2);
        let actual: (String, usize) = strip_illegal("a\u{FFFE}\u{FFFF}b");
        assert_eq!(expected, actual);
    }

    #[test]
    fn astral_characters_are_kept_so_emoji_in_a_title_survive() {
        let expected: (String, usize) = (String::from("ship it 🚀"), 0);
        let actual: (String, usize) = strip_illegal("ship it 🚀");
        assert_eq!(expected, actual);
    }
}
