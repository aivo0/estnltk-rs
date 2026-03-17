/// Edge case and robustness tests.
///
/// Tests unusual inputs that could cause crashes, panics, or incorrect behavior.

mod common;

use estnltk_segmentation::SegmentationPipeline;

fn pipeline() -> SegmentationPipeline {
    SegmentationPipeline::estonian()
}

#[test]
fn test_punctuation_only() {
    let text = "...!!!???";
    let r = pipeline().segment(text);
    assert_eq!(
        common::token_texts(text, &r),
        vec![".", ".", ".", "!", "!", "!", "?", "?", "?"]
    );
}

#[test]
fn test_only_newlines() {
    let text = "\n\n\n";
    let r = pipeline().segment(text);
    assert!(r.tokens.is_empty());
    assert!(r.words.is_empty());
    assert!(r.sentences.is_empty());
}

#[test]
fn test_whitespace_only() {
    let text = "   ";
    let r = pipeline().segment(text);
    assert!(r.tokens.is_empty());
    assert!(r.words.is_empty());
    assert!(r.sentences.is_empty());
}

#[test]
fn test_very_long_word() {
    let text = &"a".repeat(10000);
    let r = pipeline().segment(text);
    assert_eq!(r.tokens.len(), 1);
    assert_eq!(r.words.len(), 1);
    assert_eq!(r.sentences.len(), 1);
}

#[test]
fn test_multiple_spaces() {
    let text = "Tere    maailm";
    let r = pipeline().segment(text);
    assert_eq!(
        common::word_texts(text, &r),
        vec!["Tere", "maailm"]
    );
    assert_eq!(r.sentences.len(), 1);
}

#[test]
fn test_mixed_scripts() {
    let text = "Hello tere 123";
    let r = pipeline().segment(text);
    assert_eq!(
        common::word_texts(text, &r),
        vec!["Hello", "tere", "123"]
    );
    assert_eq!(r.sentences.len(), 1);
}

#[test]
fn test_single_punctuation() {
    let text = ".";
    let r = pipeline().segment(text);
    assert_eq!(common::token_texts(text, &r), vec!["."]);
    assert_eq!(r.sentences.len(), 1);
}

#[test]
fn test_unicode_estonian() {
    let text = "\u{00D6}\u{00F6}t\u{00F6}\u{00F6} j\u{00E4}\u{00E4}\u{00E4}\u{00E4}r \u{0161}okk \u{017E}anr";
    let r = pipeline().segment(text);
    assert_eq!(
        common::word_texts(text, &r),
        vec!["\u{00D6}\u{00F6}t\u{00F6}\u{00F6}", "j\u{00E4}\u{00E4}\u{00E4}\u{00E4}r", "\u{0161}okk", "\u{017E}anr"]
    );
    assert_eq!(r.sentences.len(), 1);
}
