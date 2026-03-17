pub mod punkt;
pub mod punkt_params;
pub mod merge_patterns;
pub mod postcorrections;

use std::collections::HashSet;

use estnltk_core::{char_to_byte_map, MatchSpan};

use crate::compound_token::CompoundToken;
use crate::word_tagger::Word;
use self::merge_patterns::{build_merge_patterns, MergePattern};
use self::punkt::sentences_from_tokens;
use self::punkt_params::PunktParameters;

/// A sentence is a sequence of word spans.
#[derive(Debug, Clone)]
pub struct Sentence {
    /// Bounding span of the sentence
    pub span: MatchSpan,
    /// The word spans that belong to this sentence
    pub word_spans: Vec<MatchSpan>,
}

/// Configuration for sentence tokenization.
#[derive(Debug, Clone)]
pub struct SentenceConfig {
    pub fix_paragraph_endings: bool,
    pub fix_compound_tokens: bool,
    pub fix_numeric: bool,
    pub fix_parentheses: bool,
    pub fix_double_quotes: bool,
    pub fix_inner_title_punct: bool,
    pub fix_repeated_ending_punct: bool,
    pub fix_double_quotes_based_on_counts: bool,
    pub use_emoticons_as_endings: bool,
}

impl Default for SentenceConfig {
    fn default() -> Self {
        Self {
            fix_paragraph_endings: true,
            fix_compound_tokens: true,
            fix_numeric: true,
            fix_parentheses: true,
            fix_double_quotes: true,
            fix_inner_title_punct: true,
            fix_repeated_ending_punct: true,
            fix_double_quotes_based_on_counts: false,
            use_emoticons_as_endings: true,
        }
    }
}

/// The sentence tokenizer.
pub struct SentenceTokenizer {
    config: SentenceConfig,
    merge_rules: Vec<MergePattern>,
}

impl SentenceTokenizer {
    pub fn new(config: SentenceConfig) -> Self {
        let all_patterns = build_merge_patterns();

        // Filter merge rules based on config
        let merge_rules: Vec<MergePattern> = all_patterns
            .into_iter()
            .filter(|p| {
                let ft = &p.fix_type;
                (config.fix_compound_tokens && ft.starts_with("abbrev"))
                    || (config.fix_repeated_ending_punct && ft.starts_with("repeated_ending_punct"))
                    || (config.fix_numeric && ft.starts_with("numeric"))
                    || (config.fix_parentheses && ft.starts_with("parentheses"))
                    || (config.fix_double_quotes && ft.starts_with("double_quotes"))
                    || (config.fix_inner_title_punct && ft.starts_with("inner_title_punct"))
            })
            .collect();

        SentenceTokenizer {
            config,
            merge_rules,
        }
    }

    /// Create with default Estonian configuration.
    pub fn estonian() -> Self {
        Self::new(SentenceConfig::default())
    }

    /// Split text into sentences.
    pub fn split_sentences(
        &self,
        text: &str,
        words: &[Word],
        compound_tokens: &[CompoundToken],
    ) -> Vec<Sentence> {
        if words.is_empty() {
            return Vec::new();
        }

        let c2b = char_to_byte_map(text);
        let params = PunktParameters::estonian();

        // Run Punkt base tokenizer using sentences_from_tokens
        let word_texts: Vec<&str> = words
            .iter()
            .map(|w| &text[c2b[w.span.start]..c2b[w.span.end]])
            .collect();
        let punkt_breaks = sentences_from_tokens(&word_texts, params);

        // Convert Punkt breaks to sentence_ends set
        let mut sentence_ends: HashSet<usize> = punkt_breaks
            .iter()
            .map(|&idx| words[idx].span.end)
            .collect();

        // A) Fix compound tokens
        if self.config.fix_compound_tokens {
            postcorrections::fix_compound_tokens(&mut sentence_ends, compound_tokens);
        }

        // B) Fix repeated ending punctuation
        if self.config.fix_repeated_ending_punct {
            postcorrections::fix_repeated_ending_punct(&mut sentence_ends, words, text, &c2b);
        }

        // C) Use emoticons as sentence endings
        if self.config.use_emoticons_as_endings {
            postcorrections::fix_emoticons_as_endings(
                &mut sentence_ends, words, compound_tokens, text, &c2b,
            );
        }

        // D) Align sentence endings with word boundaries
        let sentences_list = postcorrections::align_sentence_boundaries(&sentence_ends, words);

        // E) Split by double newlines
        let (sentences_list, fixes_list) = if self.config.fix_paragraph_endings {
            postcorrections::split_by_double_newlines(text, sentences_list, &c2b)
        } else {
            let fixes = vec![Vec::new(); sentences_list.len()];
            (sentences_list, fixes)
        };

        // F) Merge mistakenly split sentences
        let (sentences_list, _fixes_list) = if !self.merge_rules.is_empty() {
            postcorrections::merge_mistakenly_split_sentences(
                text,
                sentences_list,
                fixes_list,
                &self.merge_rules,
                self.config.fix_paragraph_endings,
                &c2b,
            )
        } else {
            (sentences_list, fixes_list)
        };

        // G) TODO: counting corrections to double quotes (not default)

        // Convert to Sentence structs
        sentences_list
            .into_iter()
            .filter(|spans| !spans.is_empty())
            .map(|spans| {
                let start = spans.first().unwrap().start;
                let end = spans.last().unwrap().end;
                Sentence {
                    span: MatchSpan::new(start, end),
                    word_spans: spans,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_sentence_split() {
        let tokenizer = SentenceTokenizer::estonian();
        let text = "Tere maailm. Kuidas l\u{00E4}heb?";
        let token_tagger = crate::tokens_tagger::TokensTagger::new();
        let tokens = token_tagger.tokenize(text);
        let words: Vec<Word> = tokens.iter().map(|&span| Word {
            span,
            normalized_form: None,
        }).collect();
        let sentences = tokenizer.split_sentences(text, &words, &[]);
        assert_eq!(sentences.len(), 2, "Expected 2 sentences");
    }
}
