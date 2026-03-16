use std::sync::OnceLock;

use regex::Regex;

use estnltk_core::MatchSpan;

use crate::compound_token::CompoundToken;
use crate::word_tagger::Word;
use super::merge_patterns::MergePattern;

/// Apply postcorrection step A: remove sentence endings inside compound tokens
/// and after non_ending_abbreviation.
pub fn fix_compound_tokens(
    sentence_ends: &mut std::collections::HashSet<usize>,
    compound_tokens: &[CompoundToken],
) {
    for ct in compound_tokens {
        if ct.pattern_type.iter().any(|t| t == "non_ending_abbreviation") {
            // Remove ALL token span ends within this compound token
            for ts in &ct.token_spans {
                sentence_ends.remove(&ts.end);
            }
        } else {
            // Remove all EXCEPT the last token span end
            for ts in ct.token_spans.iter().take(ct.token_spans.len().saturating_sub(1)) {
                sentence_ends.remove(&ts.end);
            }
        }
    }
}

/// Apply postcorrection step B: use repeated/prolonged punctuation as sentence endings.
pub fn fix_repeated_ending_punct(
    sentence_ends: &mut std::collections::HashSet<usize>,
    words: &[Word],
    text: &str,
    c2b: &[usize],
) {
    static ENDING_PUNCT_RE: OnceLock<Regex> = OnceLock::new();
    let ending_punct_re = ENDING_PUNCT_RE.get_or_init(|| Regex::new(r"^[.?!\u{2026}]+$").unwrap());
    let mut repeated_ending_punct: Vec<String> = Vec::new();

    for (wid, word) in words.iter().enumerate() {
        let word_text = &text[c2b[word.span.start]..c2b[word.span.end]];

        if ending_punct_re.is_match(word_text) {
            repeated_ending_punct.push(word_text.to_string());
        } else if !repeated_ending_punct.is_empty() {
            repeated_ending_punct.clear();
        }

        // Check if punctuation has some length
        let has_significant_punct = repeated_ending_punct.len() > 1
            || (repeated_ending_punct.len() == 1
                && (repeated_ending_punct[0] == "\u{2026}" || repeated_ending_punct[0].len() > 1));

        if has_significant_punct {
            if wid + 1 < words.len() {
                let next_text = &text[c2b[words[wid + 1].span.start]..c2b[words[wid + 1].span.end]];
                let next_chars: Vec<char> = next_text.chars().collect();
                if next_chars.len() > 1
                    && next_chars[0].is_uppercase()
                    && (next_chars[1].is_lowercase() || next_text.chars().all(|c| c.is_uppercase()))
                {
                    sentence_ends.insert(word.span.end);
                    // Check if token before punctuation is [ ( or "
                    if wid >= repeated_ending_punct.len() {
                        let prev_word = &words[wid - repeated_ending_punct.len()];
                        let prev_text = &text[c2b[prev_word.span.start]..c2b[prev_word.span.end]];
                        if prev_text == "[" || prev_text == "(" || prev_text == "\"" {
                            sentence_ends.remove(&word.span.end);
                        }
                    }
                }
            }
        }
    }
}

/// Apply postcorrection step C: use emoticons as sentence endings.
pub fn fix_emoticons_as_endings(
    sentence_ends: &mut std::collections::HashSet<usize>,
    words: &[Word],
    compound_tokens: &[CompoundToken],
    text: &str,
    c2b: &[usize],
) {
    // Collect emoticon start locations
    let mut emoticon_starts: std::collections::HashMap<usize, &CompoundToken> =
        std::collections::HashMap::new();
    for ct in compound_tokens {
        if ct.pattern_type.iter().any(|t| t == "emoticon") {
            emoticon_starts.insert(ct.span.start, ct);
        }
    }

    let mut repeated_emoticons: Vec<&CompoundToken> = Vec::new();

    for (wid, word) in words.iter().enumerate() {
        if let Some(ct) = emoticon_starts.get(&word.span.start) {
            repeated_emoticons.push(ct);
        } else {
            repeated_emoticons.clear();
        }

        if !repeated_emoticons.is_empty() {
            if wid + 1 < words.len() {
                let next_start = words[wid + 1].span.start;
                let next_text = &text[c2b[words[wid + 1].span.start]..c2b[words[wid + 1].span.end]];
                let next_chars: Vec<char> = next_text.chars().collect();

                if !emoticon_starts.contains_key(&next_start)
                    && next_chars.len() > 1
                    && next_chars[0].is_uppercase()
                    && next_chars[1].is_lowercase()
                {
                    sentence_ends.insert(word.span.end);
                    // Remove ending of word before emoticons
                    if wid >= repeated_emoticons.len() {
                        let prev_word = &words[wid - repeated_emoticons.len()];
                        sentence_ends.remove(&prev_word.span.end);
                    }
                }
            }
        }
    }
}

/// Apply postcorrection step D: align sentence endings with word boundaries.
/// Returns a list of sentences, where each sentence is a list of word spans.
pub fn align_sentence_boundaries(
    sentence_ends: &std::collections::HashSet<usize>,
    words: &[Word],
) -> Vec<Vec<MatchSpan>> {
    if words.is_empty() {
        return Vec::new();
    }

    let mut all_ends = sentence_ends.clone();
    all_ends.insert(words.last().unwrap().span.end);

    let mut sentences = Vec::new();
    let mut start = 0;

    for (i, word) in words.iter().enumerate() {
        if all_ends.contains(&word.span.end) {
            let sentence_spans: Vec<MatchSpan> = words[start..=i]
                .iter()
                .map(|w| w.span)
                .collect();
            sentences.push(sentence_spans);
            start = i + 1;
        }
    }

    sentences
}

/// Apply postcorrection step E: split sentences by double newlines.
pub fn split_by_double_newlines(
    text: &str,
    sentences: Vec<Vec<MatchSpan>>,
    c2b: &[usize],
) -> (Vec<Vec<MatchSpan>>, Vec<Vec<String>>) {
    let double_newline = "\n\n";
    let mut new_sentences = Vec::new();
    let mut fixes_list = Vec::new();

    for sentence_spans in sentences {
        if sentence_spans.is_empty() {
            continue;
        }
        let sent_start = sentence_spans.first().unwrap().start;
        let sent_end = sentence_spans.last().unwrap().end;
        let sent_text = &text[c2b[sent_start]..c2b[sent_end]];

        if sent_text.contains(double_newline) {
            let mut current_words = Vec::new();
            for (wid, &span) in sentence_spans.iter().enumerate() {
                current_words.push(span);
                if wid + 1 < sentence_spans.len() {
                    let next_span = sentence_spans[wid + 1];
                    let space_between = &text[c2b[span.end]..c2b[next_span.start]];
                    if space_between.contains(double_newline) {
                        new_sentences.push(current_words.clone());
                        fixes_list.push(vec!["double_newline_ending".to_string()]);
                        current_words.clear();
                    }
                }
            }
            if !current_words.is_empty() {
                new_sentences.push(current_words);
                fixes_list.push(Vec::new());
            }
        } else {
            new_sentences.push(sentence_spans);
            fixes_list.push(Vec::new());
        }
    }

    (new_sentences, fixes_list)
}

/// Apply postcorrection step F: merge mistakenly split sentences using merge patterns.
pub fn merge_mistakenly_split_sentences(
    text: &str,
    sentences: Vec<Vec<MatchSpan>>,
    sentence_fixes: Vec<Vec<String>>,
    merge_rules: &[MergePattern],
    fix_paragraph_endings: bool,
    c2b: &[usize],
) -> (Vec<Vec<MatchSpan>>, Vec<Vec<String>>) {
    assert_eq!(sentences.len(), sentence_fixes.len());
    let mut new_sentences: Vec<Vec<MatchSpan>> = Vec::new();
    let mut new_fixes: Vec<Vec<String>> = Vec::new();

    for (sid, sentence_spl) in sentences.iter().enumerate() {
        let this_fixes = &sentence_fixes[sid];
        let this_sent_start = sentence_spl.first().map(|s| s.start).unwrap_or(0);
        let this_sent_end = sentence_spl.last().map(|s| s.end).unwrap_or(0);
        let this_sent = &text[c2b[this_sent_start]..c2b[this_sent_end]];
        let this_sent_trimmed = this_sent.trim_start();

        let mut merge = false;
        let mut current_fix_types: Vec<String> = Vec::new();
        let mut shift_ending: Option<(usize, usize)> = None;

        if sid > 0 {
            let prev_spl: &Vec<MatchSpan> = if !new_sentences.is_empty() {
                new_sentences.last().unwrap()
            } else {
                &sentences[sid - 1]
            };
            let _prev_fixes: &Vec<String> = if !new_fixes.is_empty() {
                new_fixes.last().unwrap()
            } else {
                &sentence_fixes[sid - 1]
            };

            let prev_start = prev_spl.first().map(|s| s.start).unwrap_or(0);
            let prev_end = prev_spl.last().map(|s| s.end).unwrap_or(0);
            let prev_sent = &text[c2b[prev_start]..c2b[prev_end]];
            let prev_sent_trimmed = prev_sent.trim_end();

            let mut discard_merge = false;
            if fix_paragraph_endings {
                let between = &text[c2b[prev_end]..c2b[this_sent_start]];
                if between.contains("\n\n") {
                    discard_merge = true;
                }
            }

            if !discard_merge {
                for pattern in merge_rules {
                    if pattern.end_matches(this_sent_trimmed)
                        && pattern.begin_pat.is_match(prev_sent_trimmed)
                    {
                        merge = true;
                        current_fix_types.push(pattern.fix_type.clone());

                        if pattern.shift_end {
                            shift_ending = find_new_sentence_ending(
                                text, pattern, sentence_spl, prev_spl, c2b,
                            );
                        }
                        break;
                    }
                }
            }
        }

        if merge {
            if let Some(end_span) = shift_ending {
                // Merge-and-split
                let prev_ref = if !new_sentences.is_empty() {
                    new_sentences.last().unwrap().clone()
                } else {
                    sentences[sid.saturating_sub(1)].clone()
                };
                if let Some(result) = perform_merge_split(end_span, sentence_spl, &prev_ref) {
                    if !new_sentences.is_empty() {
                        let prev_f = new_fixes.last().unwrap().clone();
                        *new_sentences.last_mut().unwrap() = result.0;
                        *new_fixes.last_mut().unwrap() = [prev_f, current_fix_types.clone()].concat();
                        new_sentences.push(result.1);
                        new_fixes.push([this_fixes.clone(), current_fix_types].concat());
                    } else {
                        new_sentences.push(result.0);
                        new_fixes.push([sentence_fixes[sid.saturating_sub(1)].clone(), current_fix_types.clone()].concat());
                        new_sentences.push(result.1);
                        new_fixes.push([this_fixes.clone(), current_fix_types].concat());
                    }
                } else {
                    new_sentences.push(sentence_spl.clone());
                    new_fixes.push(this_fixes.clone());
                }
            } else {
                // Simple merge
                if new_sentences.is_empty() {
                    let mut merged = sentences[sid - 1].clone();
                    merged.extend_from_slice(sentence_spl);
                    new_sentences.push(merged);
                    let all_fixes = [
                        sentence_fixes[sid - 1].clone(),
                        this_fixes.clone(),
                        current_fix_types,
                    ].concat();
                    new_fixes.push(all_fixes);
                } else {
                    new_sentences.last_mut().unwrap().extend_from_slice(sentence_spl);
                    let prev_f = new_fixes.last().unwrap().clone();
                    *new_fixes.last_mut().unwrap() = [prev_f, this_fixes.clone(), current_fix_types].concat();
                }
            }
        } else {
            new_sentences.push(sentence_spl.clone());
            new_fixes.push(this_fixes.clone());
        }
    }

    assert_eq!(new_sentences.len(), new_fixes.len());
    (new_sentences, new_fixes)
}

/// Find a new sentence ending position for merge-and-split operations.
fn find_new_sentence_ending(
    text: &str,
    pattern: &MergePattern,
    this_sent: &[MatchSpan],
    prev_sent: &[MatchSpan],
    c2b: &[usize],
) -> Option<(usize, usize)> {
    if !pattern.shift_end {
        return None;
    }

    // Check end pattern for named group <end>
    if pattern.end_pat.as_str().contains("?P<end>") {
        let this_start = this_sent.first()?.start;
        let this_end = this_sent.last()?.end;
        let this_text = &text[c2b[this_start]..c2b[this_end]];
        let this_trimmed = this_text.trim_start();
        let trim_offset = this_text.len() - this_trimmed.len();

        if let Some(caps) = pattern.end_pat.captures(this_trimmed) {
            if let Some(end_match) = caps.name("end") {
                let end_in_text_byte = c2b[this_start] + trim_offset + end_match.end();
                // Convert byte offset back to char offset
                let b2c = estnltk_core::byte_to_char_map(text);
                let end_char = b2c[end_in_text_byte];

                // Validate that end matches a word ending
                for span in this_sent {
                    if span.end == end_char {
                        let start_char = b2c[c2b[this_start] + trim_offset + end_match.start()];
                        return Some((start_char, end_char));
                    }
                }
            }
        }
    }

    // Check begin pattern for named group <end>
    if pattern.begin_pat.as_str().contains("?P<end>") {
        let prev_start = prev_sent.first()?.start;
        let prev_end = prev_sent.last()?.end;
        let prev_text = &text[c2b[prev_start]..c2b[prev_end]];
        let prev_trimmed = prev_text.trim_start();
        let trim_offset = prev_text.len() - prev_trimmed.len();

        if let Some(caps) = pattern.begin_pat.captures(prev_trimmed) {
            if let Some(end_match) = caps.name("end") {
                let end_in_text_byte = c2b[prev_start] + trim_offset + end_match.end();
                let b2c = estnltk_core::byte_to_char_map(text);
                let end_char = b2c[end_in_text_byte];

                for span in prev_sent {
                    if span.end == end_char {
                        let start_char = b2c[c2b[prev_start] + trim_offset + end_match.start()];
                        return Some((start_char, end_char));
                    }
                }
            }
        }
    }

    None
}

/// Perform a merge-and-split operation on consecutive sentences.
fn perform_merge_split(
    end_span: (usize, usize),
    this_sent: &[MatchSpan],
    prev_sent: &[MatchSpan],
) -> Option<(Vec<MatchSpan>, Vec<MatchSpan>)> {
    let mut new_sentence1 = Vec::new();
    let mut new_sentence2 = Vec::new();

    for &span in prev_sent {
        if span.end <= end_span.1 {
            new_sentence1.push(span);
        } else if span.start >= end_span.1 {
            new_sentence2.push(span);
        }
    }
    for &span in this_sent {
        if span.end <= end_span.1 {
            new_sentence1.push(span);
        } else if span.start >= end_span.1 {
            new_sentence2.push(span);
        }
    }

    // Validity check
    if prev_sent.len() + this_sent.len() != new_sentence1.len() + new_sentence2.len() {
        return None;
    }
    if new_sentence1.is_empty() || new_sentence2.is_empty() {
        return None;
    }

    Some((new_sentence1, new_sentence2))
}
