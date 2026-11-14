use super::entity::Entity;
use super::utf16::utf16_len;

/// Room reserved for a `(1/3)` style indicator when a message is split.
const INDICATOR_RESERVE: usize = 12;

/// One message-sized piece of a larger message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Chunk {
    pub(crate) text: String,
    pub(crate) entities: Vec<Entity>,
}

/// The result of fitting a message into Telegram's length limit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SplitOutcome {
    /// The pieces to send, in order.
    pub(crate) chunks: Vec<Chunk>,
    /// How many UTF-16 code units were discarded because the chunk cap was hit.
    pub(crate) dropped_units: usize,
}

/// Returns the byte index of the longest prefix of `text` fitting in `limit`
/// UTF-16 code units, preferring to cut at a natural boundary.
///
/// Preference order is paragraph break, then line break, then space, then a
/// hard cut at the limit. The returned index is always on a `char` boundary,
/// so a surrogate pair can never be severed.
fn find_cut(text: &str, limit: usize) -> usize {
    let mut units: usize = 0;
    let mut paragraph_end: usize = 0;
    let mut line_end: usize = 0;
    let mut space_end: usize = 0;
    let mut hard_end: usize = 0;
    let mut previous_was_newline: bool = false;

    for (byte_index, character) in text.char_indices() {
        let character_units: usize = character.len_utf16();

        if units + character_units > limit {
            break;
        }

        units += character_units;
        let end: usize = byte_index + character.len_utf8();
        hard_end = end;

        if character == '\n' {
            if previous_was_newline {
                paragraph_end = end;
            }
            line_end = end;
            previous_was_newline = true;
        } else {
            if character == ' ' {
                space_end = end;
            }
            previous_was_newline = false;
        }
    }

    // Everything fits.
    if hard_end == text.len() {
        return text.len();
    }

    let cut: usize = if paragraph_end > 0 {
        paragraph_end
    } else if line_end > 0 {
        line_end
    } else if space_end > 0 {
        space_end
    } else {
        hard_end
    };

    if cut > 0 {
        return cut;
    }

    // Defensive: a limit smaller than the first character would otherwise make
    // no progress and loop forever. Overshoot by one character instead.
    text.char_indices()
        .nth(1)
        .map_or_else(|| text.len(), |(index, _)| index)
}

/// Returns the byte index at which `text` reaches `units` UTF-16 code units.
fn byte_index_for_units(text: &str, units: usize) -> usize {
    let mut seen: usize = 0;

    for (byte_index, character) in text.char_indices() {
        if seen + character.len_utf16() > units {
            return byte_index;
        }
        seen += character.len_utf16();
    }

    text.len()
}

/// Restricts `entities` to the absolute UTF-16 range `[start, end)`, clamping
/// any which straddle a boundary and rebasing them onto the new chunk.
fn clamp_entities(entities: &[Entity], start: usize, end: usize) -> Vec<Entity> {
    entities
        .iter()
        .filter_map(|entity: &Entity| {
            let clamped_start: usize = entity.offset.max(start);
            let clamped_end: usize = entity.end().min(end);

            if clamped_start >= clamped_end {
                return None;
            }

            Some(Entity::new(
                entity.kind.clone(),
                clamped_start - start,
                clamped_end - clamped_start,
            ))
        })
        .collect()
}

/// Splits text and its entities into pieces which each fit inside `limit`.
///
/// At most `max_chunks` pieces are produced; anything beyond that is reported
/// as `dropped_units` rather than sent, so that a runaway upstream message
/// cannot turn a single notification into an unbounded flood.
fn split_once(text: &str, entities: &[Entity], limit: usize, max_chunks: usize) -> SplitOutcome {
    if utf16_len(text) <= limit {
        return SplitOutcome {
            chunks: vec![Chunk {
                text: text.to_string(),
                entities: entities.to_vec(),
            }],
            dropped_units: 0,
        };
    }

    let mut chunks: Vec<Chunk> = Vec::new();
    let mut remainder: &str = text;
    let mut absolute_offset: usize = 0;

    while !remainder.is_empty() && chunks.len() < max_chunks {
        let cut: usize = find_cut(remainder, limit);
        let piece: &str = &remainder[..cut];
        let piece_units: usize = utf16_len(piece);

        chunks.push(Chunk {
            text: piece.trim_end().to_string(),
            entities: clamp_entities(
                entities,
                absolute_offset,
                absolute_offset + utf16_len(piece.trim_end()),
            ),
        });

        absolute_offset += piece_units;
        remainder = &remainder[cut..];

        // Whitespace at a split point would otherwise open the next chunk with
        // a blank line. Skipping it also skips any entity covering only it.
        let trimmed: &str = remainder.trim_start();
        absolute_offset += utf16_len(remainder) - utf16_len(trimmed);
        remainder = trimmed;
    }

    // A chunk which was nothing but whitespace would be rejected by Telegram.
    chunks.retain(|chunk: &Chunk| !chunk.text.is_empty());

    SplitOutcome {
        chunks,
        dropped_units: utf16_len(remainder),
    }
}

/// Shortens a chunk to `units` UTF-16 code units, clamping its entities.
fn truncate_chunk(chunk: &Chunk, units: usize) -> Chunk {
    if utf16_len(&chunk.text) <= units {
        return chunk.clone();
    }

    let cut: usize = byte_index_for_units(&chunk.text, units);
    let text: String = chunk.text[..cut].trim_end().to_string();
    let entities: Vec<Entity> = clamp_entities(&chunk.entities, 0, utf16_len(&text));

    Chunk { text, entities }
}

/// Prefixes `text` onto a chunk, rebasing its entities past the prefix.
fn prefix_chunk(chunk: &Chunk, prefix: &str) -> Chunk {
    let shift: usize = utf16_len(prefix);

    Chunk {
        text: format!("{prefix}{}", chunk.text),
        entities: chunk
            .entities
            .iter()
            .map(|entity: &Entity| {
                Entity::new(entity.kind.clone(), entity.offset + shift, entity.length)
            })
            .collect(),
    }
}

/// Fits a message into Telegram's length limit, splitting it if necessary.
///
/// When the message splits, each piece is prefixed with a `(1/3)` indicator so
/// that a sequence of arriving messages reads as one message rather than as
/// several unrelated ones. When the chunk cap is reached, the final piece
/// carries a visible marker naming how much was dropped.
pub(crate) fn prepare(
    text: &str,
    entities: &[Entity],
    limit: usize,
    max_chunks: usize,
) -> SplitOutcome {
    let first_pass: SplitOutcome = split_once(text, entities, limit, max_chunks);

    if first_pass.chunks.len() <= 1 {
        return first_pass;
    }

    // Splitting is happening, so every chunk needs room for its indicator.
    let effective_limit: usize = limit.saturating_sub(INDICATOR_RESERVE).max(1);
    let outcome: SplitOutcome = split_once(text, entities, effective_limit, max_chunks);
    let total: usize = outcome.chunks.len();

    let mut chunks: Vec<Chunk> = outcome
        .chunks
        .iter()
        .enumerate()
        .map(|(index, chunk): (usize, &Chunk)| {
            prefix_chunk(chunk, &format!("({}/{total}) ", index + 1))
        })
        .collect();

    if outcome.dropped_units > 0
        && let Some(last) = chunks.last_mut()
    {
        let marker: String = format!(
            "\n… truncated, {} more characters dropped",
            outcome.dropped_units
        );
        let room: usize = limit.saturating_sub(utf16_len(&marker));
        let mut truncated: Chunk = truncate_chunk(last, room);
        truncated.text.push_str(&marker);
        *last = truncated;
    }

    SplitOutcome {
        chunks,
        dropped_units: outcome.dropped_units,
    }
}

#[cfg(test)]
mod tests {
    use super::{Chunk, SplitOutcome, find_cut, prepare, split_once};
    use crate::telegram::entity::{Entity, EntityKind};
    use crate::telegram::utf16::utf16_len;

    #[test]
    fn short_text_is_a_single_chunk() {
        let outcome: SplitOutcome = split_once("hello", &[], 4096, 5);

        let expected: usize = 1;
        let actual: usize = outcome.chunks.len();
        assert_eq!(expected, actual);

        let expected_text: String = String::from("hello");
        let actual_text: String = outcome.chunks[0].text.clone();
        assert_eq!(expected_text, actual_text);

        let expected_dropped: usize = 0;
        let actual_dropped: usize = outcome.dropped_units;
        assert_eq!(expected_dropped, actual_dropped);
    }

    #[test]
    fn entities_survive_an_unsplit_message() {
        let entities: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 0, 5)];
        let outcome: SplitOutcome = split_once("hello", &entities, 4096, 5);

        let expected: Vec<Entity> = entities.clone();
        let actual: Vec<Entity> = outcome.chunks[0].entities.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn long_text_splits_into_multiple_chunks() {
        let text: String = "a".repeat(10_000);
        let outcome: SplitOutcome = split_once(&text, &[], 4096, 5);

        let expected: usize = 3;
        let actual: usize = outcome.chunks.len();
        assert_eq!(expected, actual);

        let expected_dropped: usize = 0;
        let actual_dropped: usize = outcome.dropped_units;
        assert_eq!(expected_dropped, actual_dropped);
    }

    #[test]
    fn no_chunk_exceeds_the_limit() {
        let text: String = "lorem ipsum dolor sit amet ".repeat(500);
        let outcome: SplitOutcome = split_once(&text, &[], 100, 50);

        for chunk in &outcome.chunks {
            assert!(utf16_len(&chunk.text) <= 100);
        }
    }

    #[test]
    fn splitting_prefers_line_boundaries() {
        let text: String = format!("{}\n{}", "a".repeat(50), "b".repeat(50));
        let outcome: SplitOutcome = split_once(&text, &[], 60, 5);

        let expected_first: String = "a".repeat(50);
        let actual_first: String = outcome.chunks[0].text.clone();
        assert_eq!(expected_first, actual_first);

        let expected_second: String = "b".repeat(50);
        let actual_second: String = outcome.chunks[1].text.clone();
        assert_eq!(expected_second, actual_second);
    }

    #[test]
    fn splitting_never_severs_a_surrogate_pair() {
        // Each 🎨 is 2 UTF-16 code units; an odd limit forces a cut which a
        // naive implementation would place inside a pair.
        let text: String = "🎨".repeat(100);
        let outcome: SplitOutcome = split_once(&text, &[], 51, 20);

        for chunk in &outcome.chunks {
            // A severed pair would not round-trip through char boundaries.
            let expected: String = chunk.text.clone();
            let actual: String = chunk.text.chars().collect::<String>();
            assert_eq!(expected, actual);
            assert!(utf16_len(&chunk.text) <= 51);
        }

        let expected_total: usize = 100;
        let actual_total: usize = outcome
            .chunks
            .iter()
            .map(|chunk: &Chunk| chunk.text.chars().count())
            .sum();
        assert_eq!(expected_total, actual_total);
    }

    #[test]
    fn entities_straddling_a_boundary_are_clamped_into_both_chunks() {
        // 100 'a's, with a bold run covering offsets 40..60, split at 50.
        let text: String = "a".repeat(100);
        let entities: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 40, 20)];
        let outcome: SplitOutcome = split_once(&text, &entities, 50, 5);

        let expected_first: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 40, 10)];
        let actual_first: Vec<Entity> = outcome.chunks[0].entities.clone();
        assert_eq!(expected_first, actual_first);

        let expected_second: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 0, 10)];
        let actual_second: Vec<Entity> = outcome.chunks[1].entities.clone();
        assert_eq!(expected_second, actual_second);
    }

    #[test]
    fn entities_entirely_outside_a_chunk_are_dropped() {
        let text: String = "a".repeat(100);
        let entities: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 60, 10)];
        let outcome: SplitOutcome = split_once(&text, &entities, 50, 5);

        let expected_first: Vec<Entity> = Vec::new();
        let actual_first: Vec<Entity> = outcome.chunks[0].entities.clone();
        assert_eq!(expected_first, actual_first);

        let expected_second: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 10, 10)];
        let actual_second: Vec<Entity> = outcome.chunks[1].entities.clone();
        assert_eq!(expected_second, actual_second);
    }

    #[test]
    fn hitting_the_chunk_cap_reports_dropped_units() {
        let text: String = "a".repeat(1000);
        let outcome: SplitOutcome = split_once(&text, &[], 100, 5);

        let expected_chunks: usize = 5;
        let actual_chunks: usize = outcome.chunks.len();
        assert_eq!(expected_chunks, actual_chunks);

        let expected_dropped: usize = 500;
        let actual_dropped: usize = outcome.dropped_units;
        assert_eq!(expected_dropped, actual_dropped);
    }

    #[test]
    fn find_cut_returns_whole_string_when_it_fits() {
        let expected: usize = 5;
        let actual: usize = find_cut("hello", 10);
        assert_eq!(expected, actual);
    }

    #[test]
    fn find_cut_always_makes_progress() {
        // A limit below the width of the first character must still advance.
        let actual: usize = find_cut("🎨🎨", 1);
        assert!(actual > 0);
    }

    #[test]
    fn prepare_adds_indicators_when_splitting() {
        let text: String = "a".repeat(300);
        let outcome: SplitOutcome = prepare(&text, &[], 100, 5);

        assert!(outcome.chunks[0].text.starts_with("(1/"));
        assert!(outcome.chunks[1].text.starts_with("(2/"));

        for chunk in &outcome.chunks {
            assert!(utf16_len(&chunk.text) <= 100);
        }
    }

    #[test]
    fn prepare_adds_no_indicator_to_a_single_chunk() {
        let outcome: SplitOutcome = prepare("hello", &[], 100, 5);

        let expected: String = String::from("hello");
        let actual: String = outcome.chunks[0].text.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn prepare_shifts_entities_past_the_indicator() {
        let text: String = "a".repeat(300);
        let entities: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 0, 10)];
        let outcome: SplitOutcome = prepare(&text, &entities, 100, 5);

        // "(1/4) " is 6 UTF-16 code units.
        let expected: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 6, 10)];
        let actual: Vec<Entity> = outcome.chunks[0].entities.clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn prepare_marks_truncation_in_the_final_chunk() {
        let text: String = "a".repeat(10_000);
        let outcome: SplitOutcome = prepare(&text, &[], 100, 3);

        assert!(outcome.dropped_units > 0);

        let last: String = outcome
            .chunks
            .last()
            .expect("there should be chunks")
            .text
            .clone();
        assert!(last.contains("truncated"));
        assert!(last.contains("more characters dropped"));

        for chunk in &outcome.chunks {
            assert!(utf16_len(&chunk.text) <= 100);
        }
    }

    #[test]
    fn prepare_respects_the_caption_limit() {
        let text: String = "a".repeat(5000);
        let outcome: SplitOutcome = prepare(&text, &[], 1024, 5);

        for chunk in &outcome.chunks {
            assert!(utf16_len(&chunk.text) <= 1024);
        }
    }
}
