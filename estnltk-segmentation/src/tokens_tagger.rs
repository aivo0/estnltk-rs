use estnltk_core::{byte_to_char_map, MatchSpan};
use regex::Regex;

/// Tokenizes text into tokens based on whitespace and punctuation.
///
/// Port of NLTK's WordPunctTokenizer with Estonian-specific post-fixes.
pub struct TokensTagger {
    /// Main tokenizer pattern: words or punctuation sequences
    word_punct_re: Regex,
    /// Matches 2+ consecutive punctuation symbols that should be split
    punct_split_re: Regex,
    /// Matches punctuation sequences that should NOT be split (ellipsis, ?!, etc.)
    punct_no_split_re: Regex,
    /// Matches quotation mark characters
    quotes_split_re: Regex,
    /// Whether to apply punctuation post-fixes
    pub apply_punct_postfixes: bool,
    /// Whether to apply quotation mark post-fixes
    pub apply_quotes_postfixes: bool,
}

impl TokensTagger {
    pub fn new() -> Self {
        // WordPunctTokenizer pattern
        let word_punct_re = Regex::new(r"[\w]+|[^\w\s]+").unwrap();

        // Pattern for tokens that should be retokenized (2+ consecutive punctuation)
        let punct_split_re = Regex::new(
            "^[!\"#$%&'()*+,\\-./:;<=>?@^_`\\{|\\}~\\[\\]«»\u{201C}\u{201D}\u{201F}\u{201E}]{2,}$"
        ).unwrap();

        // Exceptions: ellipsis (2+ dots) or repeated ?/!
        let punct_no_split_re = Regex::new(r"^(\.{2,}|[?!]+)$").unwrap();

        // Quotation mark characters
        let quotes_split_re = Regex::new(
            "[\"\u{00AB}\u{00BB}\u{02EE}\u{030B}\u{201C}\u{201D}\u{201E}\u{201F}]+"
        ).unwrap();

        Self {
            word_punct_re,
            punct_split_re,
            punct_no_split_re,
            quotes_split_re,
            apply_punct_postfixes: true,
            apply_quotes_postfixes: true,
        }
    }

    /// Tokenize text into character-level spans.
    pub fn tokenize(&self, text: &str) -> Vec<MatchSpan> {
        let b2c = byte_to_char_map(text);

        // Initial tokenization using WordPunctTokenizer pattern
        let mut spans: Vec<MatchSpan> = self.word_punct_re
            .find_iter(text)
            .map(|m| MatchSpan::new(b2c[m.start()], b2c[m.end()]))
            .collect();

        // We need char_to_byte for slicing text by char spans
        let c2b = estnltk_core::char_to_byte_map(text);

        // Punct postfix: split multi-punctuation tokens
        if self.apply_punct_postfixes {
            let mut spans_to_split = Vec::new();
            for &span in &spans {
                let token = &text[c2b[span.start]..c2b[span.end]];
                if self.punct_split_re.is_match(token)
                    && !self.punct_no_split_re.is_match(token)
                {
                    spans_to_split.push(span);
                }
            }
            if !spans_to_split.is_empty() {
                spans = self.split_into_symbols(&spans, &spans_to_split);
            }
        }

        // Quote postfix: separate quote chars from word boundaries
        if self.apply_quotes_postfixes {
            let mut q_split_map: Vec<(MatchSpan, Vec<MatchSpan>)> = Vec::new();
            for &span in &spans {
                let token = &text[c2b[span.start]..c2b[span.end]];
                if self.quotes_split_re.is_match(token) {
                    let token_char_len = token.chars().count();
                    // Collect all quotation mark matches within the token
                    let token_b2c = byte_to_char_map(token);
                    let match_locs: Vec<(usize, usize)> = self.quotes_split_re
                        .find_iter(token)
                        .map(|m| (token_b2c[m.start()], token_b2c[m.end()]))
                        .filter(|&(qs, qe)| qe - qs < token_char_len) // only sub-strings
                        .collect();

                    if match_locs.is_empty() {
                        continue;
                    }

                    let mut split_spans = Vec::new();
                    for &(q_start, q_end) in match_locs.iter() {
                        if q_start == 0 {
                            // Quote at the beginning
                            split_spans.push(MatchSpan::new(span.start + q_start, span.start + q_end));
                            if match_locs.len() == 1 {
                                // Complete: '"Euroopa' → '"', 'Euroopa'
                                split_spans.push(MatchSpan::new(span.start + q_end, span.end));
                            }
                        } else if q_end == token_char_len {
                            if match_locs.len() == 1 {
                                // Complete: '2020"' → '2020', '"'
                                split_spans.push(MatchSpan::new(span.start, span.start + q_start));
                            } else {
                                // Continue: '"Euroopa" → '"', 'Euroopa', '"'
                                let last_end = split_spans.last().map(|s: &MatchSpan| s.end).unwrap_or(span.start);
                                split_spans.push(MatchSpan::new(last_end, span.start + q_start));
                            }
                            split_spans.push(MatchSpan::new(span.start + q_start, span.end));
                        }
                    }

                    if !split_spans.is_empty() {
                        q_split_map.push((span, split_spans));
                    }
                }
            }

            if !q_split_map.is_empty() {
                let mut new_spans = Vec::new();
                for span in &spans {
                    if let Some(pos) = q_split_map.iter().position(|(s, _)| s == span) {
                        new_spans.extend_from_slice(&q_split_map[pos].1);
                    } else {
                        new_spans.push(*span);
                    }
                }
                spans = new_spans;
            }
        }

        spans
    }

    /// Split certain spans into individual character spans.
    fn split_into_symbols(&self, spans: &[MatchSpan], spans_to_split: &[MatchSpan]) -> Vec<MatchSpan> {
        let mut new_spans = Vec::new();
        for &span in spans {
            if spans_to_split.contains(&span) {
                // Split each character into its own span
                for i in span.start..span.end {
                    new_spans.push(MatchSpan::new(i, i + 1));
                }
            } else {
                new_spans.push(span);
            }
        }
        new_spans
    }
}

impl Default for TokensTagger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokenization() {
        let tagger = TokensTagger::new();
        let tokens = tagger.tokenize("Hello world");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0], MatchSpan::new(0, 5));  // "Hello"
        assert_eq!(tokens[1], MatchSpan::new(6, 11)); // "world"
    }

    #[test]
    fn test_punct_split() {
        let tagger = TokensTagger::new();
        // "a.)." should be tokenized as: "a", ".", ")", "."
        let tokens = tagger.tokenize("a.).");
        assert_eq!(tokens.len(), 4);
    }

    #[test]
    fn test_ellipsis_not_split() {
        let tagger = TokensTagger::new();
        let tokens = tagger.tokenize("wait...");
        // "wait" and "..." (ellipsis should NOT be split)
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1], MatchSpan::new(4, 7)); // "..."
    }

    #[test]
    fn test_question_excl_not_split() {
        let tagger = TokensTagger::new();
        let tokens = tagger.tokenize("really?!");
        // "really" and "?!" (should NOT be split)
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_estonian_chars() {
        let tagger = TokensTagger::new();
        let tokens = tagger.tokenize("Tüüpiline öökülm");
        assert_eq!(tokens.len(), 2);
        // Character offsets, not byte offsets
        assert_eq!(tokens[0], MatchSpan::new(0, 9));  // "Tüüpiline"
        assert_eq!(tokens[1], MatchSpan::new(10, 16)); // "öökülm" (6 chars)
    }

    #[test]
    fn test_quote_separation() {
        let tagger = TokensTagger::new();
        // Quote at start of word should be separated
        let tokens = tagger.tokenize("\u{201E}Euroopa");
        assert_eq!(tokens.len(), 2);
    }
}
