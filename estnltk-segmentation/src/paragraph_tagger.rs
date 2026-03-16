use std::sync::OnceLock;

use regex::Regex;

use estnltk_core::MatchSpan;

use crate::sentence::Sentence;

/// A paragraph is a group of consecutive sentences.
#[derive(Debug, Clone)]
pub struct Paragraph {
    /// The bounding span of the paragraph (from first sentence start to last sentence end)
    pub span: MatchSpan,
    /// Indices into the sentences array that belong to this paragraph
    pub sentence_indices: Vec<usize>,
}

/// Detect paragraphs by grouping sentences at `\n\n` boundaries.
///
/// Port of Python's ParagraphTokenizer.
pub fn detect_paragraphs(
    text: &str,
    sentences: &[Sentence],
) -> Vec<Paragraph> {
    if sentences.is_empty() {
        return Vec::new();
    }

    static PARA_RE: OnceLock<Regex> = OnceLock::new();
    let para_re = PARA_RE.get_or_init(|| Regex::new(r"\s*\n\n").unwrap());

    // Find paragraph end positions (positions where paragraph-break gaps end)
    let mut paragraph_ends: std::collections::HashSet<usize> = std::collections::HashSet::new();

    // RegexpTokenizer with gaps=True: find gaps, the text between gaps are paragraphs.
    // We need the END positions of the non-gap segments (i.e., where paragraph text ends).
    // With gaps=True, the spans are the text BETWEEN the gap matches.
    // So we find all gap matches and collect the end positions of text segments.
    let b2c = estnltk_core::byte_to_char_map(text);
    let mut last_end = 0;
    for m in para_re.find_iter(text) {
        // The text segment before this gap ends at m.start()
        // But we want char offset, not byte offset
        let gap_start_char = b2c[m.start()];
        if last_end < m.start() {
            paragraph_ends.insert(gap_start_char);
        }
        last_end = m.end();
    }
    // Add the end of the last sentence as a paragraph end
    paragraph_ends.insert(sentences.last().unwrap().span.end);

    // Group sentences into paragraphs
    let mut paragraphs = Vec::new();
    let mut start_idx = 0;

    for (i, sentence) in sentences.iter().enumerate() {
        if paragraph_ends.contains(&sentence.span.end) {
            let para_span = MatchSpan::new(
                sentences[start_idx].span.start,
                sentence.span.end,
            );
            let sentence_indices: Vec<usize> = (start_idx..=i).collect();
            paragraphs.push(Paragraph {
                span: para_span,
                sentence_indices,
            });
            start_idx = i + 1;
        }
    }

    // If there are remaining sentences not ended by a paragraph break
    if start_idx < sentences.len() {
        let para_span = MatchSpan::new(
            sentences[start_idx].span.start,
            sentences.last().unwrap().span.end,
        );
        let sentence_indices: Vec<usize> = (start_idx..sentences.len()).collect();
        paragraphs.push(Paragraph {
            span: para_span,
            sentence_indices,
        });
    }

    paragraphs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_paragraph() {
        let sentences = vec![
            Sentence {
                span: MatchSpan::new(0, 10),
                word_spans: vec![MatchSpan::new(0, 4), MatchSpan::new(5, 10)],
            },
            Sentence {
                span: MatchSpan::new(11, 20),
                word_spans: vec![MatchSpan::new(11, 15), MatchSpan::new(16, 20)],
            },
        ];
        let paras = detect_paragraphs("Hello world. Good day!", &sentences);
        assert_eq!(paras.len(), 1);
    }

    #[test]
    fn test_two_paragraphs() {
        let text = "First sentence.\n\nSecond paragraph.";
        let sentences = vec![
            Sentence {
                span: MatchSpan::new(0, 15),
                word_spans: vec![MatchSpan::new(0, 5), MatchSpan::new(6, 14), MatchSpan::new(14, 15)],
            },
            Sentence {
                span: MatchSpan::new(17, 34),
                word_spans: vec![MatchSpan::new(17, 23), MatchSpan::new(24, 33), MatchSpan::new(33, 34)],
            },
        ];
        let paras = detect_paragraphs(text, &sentences);
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0].sentence_indices, vec![0]);
        assert_eq!(paras[1].sentence_indices, vec![1]);
    }
}
