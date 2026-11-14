/// Counts the length of a string in UTF-16 code units.
///
/// Telegram measures message entity offsets and lengths in UTF-16 code units,
/// **not** bytes and **not** code points: <https://core.telegram.org/api/entities>
///
/// > the length of strings when generating message entities as the number of
/// > UTF-16 code units, even if the message itself must be encoded using UTF-8.
///
/// Code points in the Basic Multilingual Plane count as 1; everything else
/// (emoji, most notably) is encoded as a surrogate pair and counts as 2.
#[must_use]
pub(crate) fn utf16_len(text: &str) -> usize {
    text.chars().map(char::len_utf16).sum()
}

/// Counts the length of a string in UTF-16 code units, ignoring trailing whitespace.
///
/// Telegram requires that entity lengths exclude trailing whitespace:
///
/// > the *length* of an entity must not include the length of trailing newlines
/// > or whitespaces, `rtrim` entities before computing their length.
#[must_use]
pub(crate) fn utf16_len_rtrimmed(text: &str) -> usize {
    utf16_len(text.trim_end())
}

#[cfg(test)]
mod tests {
    use super::{utf16_len, utf16_len_rtrimmed};

    #[test]
    fn ascii_counts_one_per_character() {
        let expected: usize = 5;
        let actual: usize = utf16_len("hello");
        assert_eq!(expected, actual);
    }

    #[test]
    fn basic_multilingual_plane_counts_one_per_character() {
        // U+00E9 (é) and U+4E2D (中) are both in the BMP.
        let expected: usize = 2;
        let actual: usize = utf16_len("é中");
        assert_eq!(expected, actual);
    }

    #[test]
    fn astral_plane_characters_count_two() {
        // U+1F3A8 (🎨) is outside the BMP, so it is a surrogate pair.
        let expected: usize = 2;
        let actual: usize = utf16_len("🎨");
        assert_eq!(expected, actual);
    }

    #[test]
    fn mixed_text_sums_correctly() {
        // "🎨 New" == 2 + 1 + 3
        let expected: usize = 6;
        let actual: usize = utf16_len("🎨 New");
        assert_eq!(expected, actual);
    }

    #[test]
    fn empty_string_is_zero() {
        let expected: usize = 0;
        let actual: usize = utf16_len("");
        assert_eq!(expected, actual);
    }

    #[test]
    fn rtrimmed_excludes_trailing_whitespace() {
        let expected: usize = 6;
        let actual: usize = utf16_len_rtrimmed("Header\n\t ");
        assert_eq!(expected, actual);
    }

    #[test]
    fn rtrimmed_keeps_leading_whitespace() {
        let expected: usize = 8;
        let actual: usize = utf16_len_rtrimmed("  Header");
        assert_eq!(expected, actual);
    }

    #[test]
    fn rtrimmed_of_only_whitespace_is_zero() {
        let expected: usize = 0;
        let actual: usize = utf16_len_rtrimmed("   \n\t");
        assert_eq!(expected, actual);
    }
}
