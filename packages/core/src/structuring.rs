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

/// Common spoken variations and aliases for Bible books in sermon transcripts
pub const BOOK_ALIASES: &[(&str, &str)] = &[
    ("1st corinthians", "1 Corinthians"),
    ("first corinthians", "1 Corinthians"),
    ("2nd corinthians", "2 Corinthians"),
    ("second corinthians", "2 Corinthians"),
    ("1st thessalonians", "1 Thessalonians"),
    ("first thessalonians", "1 Thessalonians"),
    ("2nd thessalonians", "2 Thessalonians"),
    ("second thessalonians", "2 Thessalonians"),
    ("1st timothy", "1 Timothy"),
    ("first timothy", "1 Timothy"),
    ("2nd timothy", "2 Timothy"),
    ("second timothy", "2 Timothy"),
    ("1st peter", "1 Peter"),
    ("first peter", "1 Peter"),
    ("2nd peter", "2 Peter"),
    ("second peter", "2 Peter"),
    ("1st john", "1 John"),
    ("first john", "1 John"),
    ("2nd john", "2 John"),
    ("second john", "2 John"),
    ("3rd john", "3 John"),
    ("third john", "3 John"),
    ("1st samuel", "1 Samuel"),
    ("first samuel", "1 Samuel"),
    ("2nd samuel", "2 Samuel"),
    ("second samuel", "2 Samuel"),
    ("1st kings", "1 Kings"),
    ("first kings", "1 Kings"),
    ("2nd kings", "2 Kings"),
    ("second kings", "2 Kings"),
    ("1st chronicles", "1 Chronicles"),
    ("first chronicles", "1 Chronicles"),
    ("2nd chronicles", "2 Chronicles"),
    ("second chronicles", "2 Chronicles"),
    ("song of songs", "Song of Solomon"),
    ("revelations", "Revelation"),
];

/// Common English words that also happen to be names of Bible books.
/// For these, we require an explicit chapter or verse number (e.g. "Mark 4", "Job 1:21", "Acts 2:4")
/// to avoid false positives like "mark my words", "job market", or "interacts".
const AMBIGUOUS_BOOK_NAMES: &[&str] = &["Mark", "Job", "Acts", "James", "Ruth"];

/// Detects scripture references like "John 3:16", "Romans 8:28", "Psalm 23", "First Corinthians 13:4-8" in text.
pub fn detect_scripture_references(text: &str, timestamp: f32) -> Vec<ScriptureRef> {
    let mut references = Vec::new();
    let lower_text = text.to_lowercase();

    // Prepare list of (pattern, canonical_book_name) sorted by pattern length descending
    // to match specific phrases ("first corinthians") before shorter ones ("corinthians").
    let mut patterns: Vec<(String, &'static str)> = Vec::new();
    for (alias, canonical) in BOOK_ALIASES {
        patterns.push((alias.to_string(), canonical));
    }
    for &book in BIBLE_BOOKS {
        patterns.push((book.to_lowercase(), book));
    }
    patterns.sort_by(|a, b| b.0.len().cmp(&a.0.len()));

    let mut matched_spans: Vec<(usize, usize)> = Vec::new();

    for (pat, canonical_book) in &patterns {
        let mut search_idx = 0;
        while let Some(pos) = lower_text[search_idx..].find(pat) {
            let actual_pos = search_idx + pos;
            let end_pos = actual_pos + pat.len();

            // 1. Verify start word boundary
            let start_boundary = if actual_pos == 0 {
                true
            } else {
                let prev_char = text[..actual_pos].chars().last();
                prev_char.map(|c| !c.is_alphanumeric()).unwrap_or(true)
            };

            // 2. Verify end word boundary
            let end_boundary = if end_pos >= text.len() {
                true
            } else {
                let next_char = text[end_pos..].chars().next();
                next_char.map(|c| !c.is_alphanumeric()).unwrap_or(true)
            };

            if start_boundary && end_boundary {
                // Check for overlapping span with an already matched longer pattern
                let overlaps = matched_spans
                    .iter()
                    .any(|&(s, e)| actual_pos < e && end_pos > s);

                if !overlaps {
                    let after = &text[end_pos..];
                    if let Some(ref_str) = parse_chapter_verse(after) {
                        matched_spans.push((actual_pos, end_pos));
                        references.push(ScriptureRef {
                            book: canonical_book.to_string(),
                            reference: format!("{canonical_book} {ref_str}"),
                            timestamp,
                        });
                    } else if !AMBIGUOUS_BOOK_NAMES.contains(canonical_book) {
                        // Check if book was preceded by "book of" or "epistle of" or "gospel of"
                        let prefix = text[..actual_pos].trim_end().to_lowercase();
                        if prefix.ends_with("book of")
                            || prefix.ends_with("gospel of")
                            || prefix.ends_with("epistle of")
                        {
                            matched_spans.push((actual_pos, end_pos));
                            references.push(ScriptureRef {
                                book: canonical_book.to_string(),
                                reference: canonical_book.to_string(),
                                timestamp,
                            });
                        }
                    }
                }
            }

            search_idx = actual_pos + pat.len();
        }
    }

    // Sort references by appearance order
    references.sort_by(|a, b| {
        a.reference
            .cmp(&b.reference)
    });
    references.dedup_by(|a, b| a.reference == b.reference);
    references
}

/// Helper to parse chapter:verse variations like:
/// - " 3:16", " 8:28-30", " 8:28, 30"
/// - " chapter 3 verse 16", " chapter 3, verse 16", " chapter 3:16"
/// - " 8 verse 28", " 8, verse 28", " 8 verses 28-30", " 8 v 28"
/// - " 23" (for Psalms/single chapters like Psalm 23)
fn parse_chapter_verse(s: &str) -> Option<String> {
    let lower = s.trim_start().to_lowercase();
    let trimmed = lower.as_str();

    // Check for optional "chapter " or "ch " or "ch. "
    let after_chapter = if let Some(rest) = trimmed.strip_prefix("chapter ") {
        rest.trim_start()
    } else if let Some(rest) = trimmed.strip_prefix("ch. ") {
        rest.trim_start()
    } else if let Some(rest) = trimmed.strip_prefix("ch ") {
        rest.trim_start()
    } else {
        trimmed
    };

    // Extract chapter digits
    let mut chars = after_chapter.char_indices();
    let mut chapter_digits = String::new();
    let mut end_ch_idx = 0;

    for (idx, c) in chars.by_ref() {
        if c.is_ascii_digit() {
            chapter_digits.push(c);
            end_ch_idx = idx + 1;
        } else {
            break;
        }
    }

    if chapter_digits.is_empty() {
        return None;
    }

    let remainder = after_chapter[end_ch_idx..].trim_start();

    // Check separator connecting chapter to verse
    let after_sep = if let Some(rest) = remainder.strip_prefix(':') {
        Some(rest.trim_start())
    } else if let Some(rest) = remainder.strip_prefix(", verse ") {
        Some(rest.trim_start())
    } else if let Some(rest) = remainder.strip_prefix(", verses ") {
        Some(rest.trim_start())
    } else if let Some(rest) = remainder.strip_prefix("verse ") {
        Some(rest.trim_start())
    } else if let Some(rest) = remainder.strip_prefix("verses ") {
        Some(rest.trim_start())
    } else if let Some(rest) = remainder.strip_prefix("v. ") {
        Some(rest.trim_start())
    } else if let Some(rest) = remainder.strip_prefix("v ") {
        Some(rest.trim_start())
    } else if let Some(rest) = remainder.strip_prefix(", v. ") {
        Some(rest.trim_start())
    } else if let Some(rest) = remainder.strip_prefix(", v ") {
        Some(rest.trim_start())
    } else {
        None
    };

    if let Some(verse_str) = after_sep {
        // Parse verse part: digits, dashes, commas
        let mut verse_digits = String::new();
        let mut v_chars = verse_str.chars().peekable();
        while let Some(c) = v_chars.next() {
            if c.is_ascii_digit() || c == '-' || c == ',' {
                verse_digits.push(c);
            } else if c == ' ' {
                // Check for "to " range e.g. "16 to 18"
                if v_chars.clone().take(3).collect::<String>() == "to " {
                    verse_digits.push('-');
                    v_chars.next(); // 't'
                    v_chars.next(); // 'o'
                    v_chars.next(); // ' '
                } else if v_chars.peek().map(|nc| nc.is_ascii_digit() || *nc == '-' || *nc == ',').unwrap_or(false) {
                    continue;
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let cleaned_verse = verse_digits.trim_matches(|c: char| !c.is_ascii_digit());
        if !cleaned_verse.is_empty() {
            return Some(format!("{chapter_digits}:{cleaned_verse}"));
        }
    }

    // Single chapter / Psalm reference (1-3 digits)
    if chapter_digits.len() <= 3 {
        Some(chapter_digits)
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
    fn test_detect_scripture_spoken_variations() {
        // Spoken chapter and verse
        let text1 = "Look at Romans chapter 8 verse 28 where we see all things work together.";
        let refs1 = detect_scripture_references(text1, 5.0);
        assert_eq!(refs1.len(), 1);
        assert_eq!(refs1[0].book, "Romans");
        assert_eq!(refs1[0].reference, "Romans 8:28");

        // Psalm without verse
        let text2 = "Let us read from Psalm 23 for comfort.";
        let refs2 = detect_scripture_references(text2, 12.0);
        assert_eq!(refs2.len(), 1);
        assert_eq!(refs2[0].book, "Psalm");
        assert_eq!(refs2[0].reference, "Psalm 23");

        // Spoken ordinal book aliases
        let text3 = "In First Corinthians 13:4-8 Paul describes love.";
        let refs3 = detect_scripture_references(text3, 20.0);
        assert_eq!(refs3.len(), 1);
        assert_eq!(refs3[0].book, "1 Corinthians");
        assert_eq!(refs3[0].reference, "1 Corinthians 13:4-8");

        let text4 = "Remember 2nd Timothy 1:7, God gave us a spirit of power.";
        let refs4 = detect_scripture_references(text4, 30.0);
        assert_eq!(refs4.len(), 1);
        assert_eq!(refs4[0].book, "2 Timothy");
        assert_eq!(refs4[0].reference, "2 Timothy 1:7");
    }

    #[test]
    fn test_detect_scripture_avoids_false_positives() {
        // "interacts" shouldn't match "Acts"
        // "job market" shouldn't match "Job"
        // "mark my words" shouldn't match "Mark"
        let text = "He interacts with candidates in the job market, and mark my words, they succeed.";
        let refs = detect_scripture_references(text, 15.0);
        assert_eq!(refs.len(), 0, "Common words should not trigger false positive citations");
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
