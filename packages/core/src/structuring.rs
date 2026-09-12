use crate::models::{Paragraph, ScriptureRef, Section, StructuredTranscript, TranscriptSegment};

const SECTION_MARKERS: &[&str] = &[
    "let us pray",
    "let's pray",
    "bow your heads",
    "open your bibles",
    "turn with me to",
    "turn to",
    "look at verse",
    "first point",
    "second point",
    "third point",
    "fourth point",
    "point number one",
    "point number two",
    "point number three",
    "my first point",
    "my second point",
    "in conclusion",
    "to conclude",
    "in closing",
    "finally,",
    "let us stand",
    "let's stand",
];

pub const BIBLE_BOOKS: &[&str] = &[
    "Genesis",
    "Exodus",
    "Leviticus",
    "Numbers",
    "Deuteronomy",
    "Joshua",
    "Judges",
    "Ruth",
    "1 Samuel",
    "2 Samuel",
    "1 Kings",
    "2 Kings",
    "1 Chronicles",
    "2 Chronicles",
    "Ezra",
    "Nehemiah",
    "Esther",
    "Job",
    "Psalms",
    "Psalm",
    "Proverbs",
    "Ecclesiastes",
    "Song of Solomon",
    "Isaiah",
    "Jeremiah",
    "Lamentations",
    "Ezekiel",
    "Daniel",
    "Hosea",
    "Joel",
    "Amos",
    "Obadiah",
    "Jonah",
    "Micah",
    "Nahum",
    "Habakkuk",
    "Zephaniah",
    "Haggai",
    "Zechariah",
    "Malachi",
    "Matthew",
    "Mark",
    "Luke",
    "John",
    "Acts",
    "Romans",
    "1 Corinthians",
    "2 Corinthians",
    "Galatians",
    "Ephesians",
    "Philippians",
    "Colossians",
    "1 Thessalonians",
    "2 Thessalonians",
    "1 Timothy",
    "2 Timothy",
    "Titus",
    "Philemon",
    "Hebrews",
    "James",
    "1 Peter",
    "2 Peter",
    "1 John",
    "2 John",
    "3 John",
    "Jude",
    "Revelation",
];

/// Detects scripture references like "John 3:16", "Romans 8:28", "Psalm 23:1-4" in text.
pub fn detect_scripture_references(text: &str, timestamp: f32) -> Vec<ScriptureRef> {
    let mut references = Vec::new();

    for &book in BIBLE_BOOKS {
        let lower_text = text.to_lowercase();
        let lower_book = book.to_lowercase();

        let mut search_idx = 0;
        while let Some(pos) = lower_text[search_idx..].find(&lower_book) {
            let actual_pos = search_idx + pos;
            let after = &text[actual_pos + book.len()..];

            // Look for chapter:verse pattern after book name
            if let Some(ref_str) = parse_chapter_verse(after) {
                references.push(ScriptureRef {
                    book: book.to_string(),
                    reference: format!("{book} {ref_str}"),
                    timestamp,
                });
            }

            search_idx = actual_pos + book.len();
        }
    }

    references
}

/// Helper to parse chapter:verse (e.g. " 3:16", " 8:28-30", " chapter 3 verse 16")
fn parse_chapter_verse(s: &str) -> Option<String> {
    let trimmed = s.trim_start();

    // Skip optional "chapter "
    let rest = if trimmed.to_lowercase().starts_with("chapter ") {
        &trimmed[8..]
    } else {
        trimmed
    };

    let mut result = String::new();
    let mut saw_digit = false;

    for c in rest.chars() {
        if c.is_ascii_digit() || c == ':' || c == '-' || c == ',' {
            result.push(c);
            if c.is_ascii_digit() {
                saw_digit = true;
            }
        } else if saw_digit && (c == ' ' || c == '.' || c == ';') {
            break;
        } else if !saw_digit && c == ' ' {
            continue;
        } else {
            break;
        }
    }

    let trimmed_result = result
        .trim_matches(|c: char| !c.is_ascii_digit())
        .to_string();
    if trimmed_result.contains(':') && saw_digit {
        Some(trimmed_result)
    } else if saw_digit && trimmed_result.len() <= 3 {
        // e.g. "Psalm 23"
        Some(trimmed_result)
    } else {
        None
    }
}

/// Check if a text segment contains a sermon section boundary marker.
pub fn detect_section_title(text: &str) -> Option<String> {
    let lower = text.to_lowercase();
    for &marker in SECTION_MARKERS {
        if lower.contains(marker) {
            // Capitalize marker nicely for section header
            let mut chars = marker.chars();
            let cap = match chars.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
            };
            return Some(cap);
        }
    }
    None
}

/// Applies user custom vocabulary replacements to raw transcript segments.
pub fn apply_custom_vocabulary(segments: &mut [TranscriptSegment], custom_vocab_csv: &str) {
    if custom_vocab_csv.trim().is_empty() {
        return;
    }

    let terms: Vec<&str> = custom_vocab_csv
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    for seg in segments.iter_mut() {
        for &term in &terms {
            // Case-insensitive replace
            let lower_text = seg.text.to_lowercase();
            let lower_term = term.to_lowercase();
            if lower_text.contains(&lower_term) {
                // Replace case-insensitively while preserving target casing
                let mut result = String::new();
                let mut last = 0;
                while let Some(idx) = lower_text[last..].find(&lower_term) {
                    let actual = last + idx;
                    result.push_str(&seg.text[last..actual]);
                    result.push_str(term);
                    last = actual + lower_term.len();
                }
                result.push_str(&seg.text[last..]);
                seg.text = result;
            }
        }
    }
}

/// Groups raw Whisper segments into natural paragraphs and collapsible sections.
///
/// Rules:
/// - A new paragraph is created when the silence between segments exceeds 1.2 seconds,
///   or when a segment ends in sentence punctuation (`.`, `!`, `?`) and pause > 0.8s.
/// - A new section is created when a segment matches a structural marker (e.g. "Let us pray", "In conclusion").
pub fn structure_transcript(
    raw_segments: &[TranscriptSegment],
    custom_vocab_csv: &str,
) -> StructuredTranscript {
    if raw_segments.is_empty() {
        return StructuredTranscript {
            sections: Vec::new(),
            scripture_references: Vec::new(),
        };
    }

    let mut segments = raw_segments.to_vec();
    apply_custom_vocabulary(&mut segments, custom_vocab_csv);

    let mut all_scriptures = Vec::new();
    let mut sections: Vec<Section> = Vec::new();

    let mut current_section_title = "Opening & Introduction".to_string();
    let mut current_section_start = segments[0].start;
    let mut current_paragraphs: Vec<Paragraph> = Vec::new();

    let mut current_para_segments: Vec<TranscriptSegment> = Vec::new();
    let mut current_para_text = String::new();
    let mut current_para_start = segments[0].start;

    for i in 0..segments.len() {
        let seg = &segments[i];
        let next_seg = segments.get(i + 1);

        // Check for scripture reference
        let sc_refs = detect_scripture_references(&seg.text, seg.start);
        all_scriptures.extend(sc_refs);

        // If this segment introduces a new section, seal preceding section first
        if let Some(new_title) = detect_section_title(&seg.text) {
            // First seal any in-progress paragraph
            if !current_para_text.is_empty() {
                let para_end = current_para_segments
                    .last()
                    .map(|s| s.end)
                    .unwrap_or(seg.start);
                let para_scriptures =
                    detect_scripture_references(&current_para_text, current_para_start);
                current_paragraphs.push(Paragraph {
                    start_time: current_para_start,
                    end_time: para_end,
                    text: current_para_text.clone(),
                    segments: std::mem::take(&mut current_para_segments),
                    scripture_refs: para_scriptures,
                });
                current_para_text.clear();
            }

            // Seal previous section if it has paragraphs
            if !current_paragraphs.is_empty() {
                let section_end = current_paragraphs
                    .last()
                    .map(|p| p.end_time)
                    .unwrap_or(seg.start);
                sections.push(Section {
                    title: current_section_title,
                    start_time: current_section_start,
                    end_time: section_end,
                    paragraphs: std::mem::take(&mut current_paragraphs),
                });
            }

            current_section_title = new_title;
            current_section_start = seg.start;
            current_para_start = seg.start;
        }

        current_para_segments.push(seg.clone());
        if !current_para_text.is_empty() {
            current_para_text.push(' ');
        }
        current_para_text.push_str(seg.text.trim());

        // Determine if paragraph break should happen
        let should_break_paragraph = if let Some(next) = next_seg {
            let pause = next.start - seg.end;
            let ends_sentence = seg.text.trim_end().ends_with('.')
                || seg.text.trim_end().ends_with('!')
                || seg.text.trim_end().ends_with('?');
            let next_is_section = detect_section_title(&next.text).is_some();

            pause >= 1.2 || (ends_sentence && pause >= 0.8) || next_is_section
        } else {
            true // Last segment
        };

        if should_break_paragraph {
            let end_time = seg.end;
            let para_scriptures =
                detect_scripture_references(&current_para_text, current_para_start);

            current_paragraphs.push(Paragraph {
                start_time: current_para_start,
                end_time,
                text: current_para_text.clone(),
                segments: std::mem::take(&mut current_para_segments),
                scripture_refs: para_scriptures,
            });

            current_para_text.clear();
            if let Some(next) = next_seg {
                current_para_start = next.start;
            }
        }
    }

    // Seal remaining paragraphs into last section
    if !current_paragraphs.is_empty() {
        let last_end = current_paragraphs.last().map(|p| p.end_time).unwrap_or(0.0);
        sections.push(Section {
            title: current_section_title,
            start_time: current_section_start,
            end_time: last_end,
            paragraphs: current_paragraphs,
        });
    }

    StructuredTranscript {
        sections,
        scripture_references: all_scriptures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_scripture_references() {
        let text = "Turn with me to John 3:16 where Jesus reveals the love of God.";
        let refs = detect_scripture_references(text, 10.0);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].book, "John");
        assert_eq!(refs[0].reference, "John 3:16");
    }

    #[test]
    fn test_structure_transcript_groups_paragraphs() {
        let segs = vec![
            TranscriptSegment {
                start: 0.0,
                end: 3.0,
                text: "Good morning church.".to_string(),
            },
            TranscriptSegment {
                start: 3.2,
                end: 6.0,
                text: "It is wonderful to be here today.".to_string(),
            },
            // Gap > 1.2s -> paragraph break
            TranscriptSegment {
                start: 8.0,
                end: 12.0,
                text: "Turn with me to Romans 8:28.".to_string(),
            },
        ];

        let structured = structure_transcript(&segs, "");
        assert_eq!(structured.sections.len(), 2);
        assert_eq!(structured.sections[0].paragraphs.len(), 1);
        assert_eq!(structured.sections[0].paragraphs[0].segments.len(), 2);
    }
}
