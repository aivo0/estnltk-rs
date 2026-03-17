/// Shared test helpers for extracting text lists from segmentation output.
///
/// These match the Python EstNLTK testing pattern of comparing word/sentence
/// text lists for cross-implementation validation.

use estnltk_core::char_to_byte_map;
use estnltk_segmentation::SegmentationResult;

/// Extract word text strings from segmentation result.
pub fn word_texts<'a>(text: &'a str, result: &SegmentationResult) -> Vec<&'a str> {
    let c2b = char_to_byte_map(text);
    result
        .words
        .iter()
        .map(|w| &text[c2b[w.span.start]..c2b[w.span.end]])
        .collect()
}

/// Extract sentence bounding text strings from segmentation result.
pub fn sentence_texts<'a>(text: &'a str, result: &SegmentationResult) -> Vec<&'a str> {
    let c2b = char_to_byte_map(text);
    result
        .sentences
        .iter()
        .map(|s| &text[c2b[s.span.start]..c2b[s.span.end]])
        .collect()
}

/// Extract token text strings from spans.
pub fn token_texts<'a>(text: &'a str, result: &SegmentationResult) -> Vec<&'a str> {
    let c2b = char_to_byte_map(text);
    result
        .tokens
        .iter()
        .map(|t| &text[c2b[t.start]..c2b[t.end]])
        .collect()
}
