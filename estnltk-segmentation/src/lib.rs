//! Estonian text segmentation pipeline.
//!
//! Splits raw text into tokens, compound tokens, words, sentences, and paragraphs.
//! Port of Python EstNLTK's text segmentation pipeline for bit-for-bit output
//! compatibility.

pub mod estonian;
pub mod tokens_tagger;
pub mod compound_token;
pub mod word_tagger;
pub mod sentence;
pub mod paragraph_tagger;

use estnltk_core::MatchSpan;

use compound_token::{CompoundToken, CompoundTokenConfig, CompoundTokenTagger};
use paragraph_tagger::{detect_paragraphs, Paragraph};
use sentence::{Sentence, SentenceConfig, SentenceTokenizer};
use tokens_tagger::TokensTagger;
use word_tagger::{assemble_words, Word};

/// Complete segmentation result for a text.
#[derive(Debug)]
pub struct SegmentationResult {
    pub tokens: Vec<MatchSpan>,
    pub compound_tokens: Vec<CompoundToken>,
    pub words: Vec<Word>,
    pub sentences: Vec<Sentence>,
    pub paragraphs: Vec<Paragraph>,
}

/// The full segmentation pipeline.
///
/// Compiles all regex patterns once at construction time.
pub struct SegmentationPipeline {
    token_tagger: TokensTagger,
    compound_token_tagger: CompoundTokenTagger,
    sentence_tokenizer: SentenceTokenizer,
}

impl SegmentationPipeline {
    /// Create a pipeline with default Estonian configuration.
    pub fn estonian() -> Self {
        Self {
            token_tagger: TokensTagger::new(),
            compound_token_tagger: CompoundTokenTagger::estonian(),
            sentence_tokenizer: SentenceTokenizer::estonian(),
        }
    }

    /// Create a pipeline with custom configuration.
    pub fn new(
        compound_config: CompoundTokenConfig,
        sentence_config: SentenceConfig,
    ) -> Self {
        Self {
            token_tagger: TokensTagger::new(),
            compound_token_tagger: CompoundTokenTagger::new(compound_config),
            sentence_tokenizer: SentenceTokenizer::new(sentence_config),
        }
    }

    /// Run the full segmentation pipeline on the given text.
    pub fn segment(&self, text: &str) -> SegmentationResult {
        // 1. Tokenize
        let tokens = self.token_tagger.tokenize(text);

        // 2. Detect compound tokens
        let compound_tokens = self.compound_token_tagger.detect(text, &tokens);

        // 3. Assemble words
        let words = assemble_words(&tokens, &compound_tokens);

        // 4. Split into sentences
        let sentences = self.sentence_tokenizer.split_sentences(text, &words, &compound_tokens);

        // 5. Detect paragraphs
        let paragraphs = detect_paragraphs(text, &sentences);

        SegmentationResult {
            tokens,
            compound_tokens,
            words,
            sentences,
            paragraphs,
        }
    }
}

/// Convenience function: tokenize text into character-level spans.
pub fn tokenize(text: &str) -> Vec<MatchSpan> {
    TokensTagger::new().tokenize(text)
}

/// Convenience function: detect compound tokens.
pub fn detect_compound_tokens(text: &str, tokens: &[MatchSpan]) -> Vec<CompoundToken> {
    CompoundTokenTagger::estonian().detect(text, tokens)
}

/// Convenience function: split text into sentences.
pub fn split_sentences(
    text: &str,
    words: &[Word],
    compound_tokens: &[CompoundToken],
) -> Vec<Sentence> {
    SentenceTokenizer::estonian().split_sentences(text, words, compound_tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_pipeline() {
        let pipeline = SegmentationPipeline::estonian();
        let result = pipeline.segment("Tere maailm. Kuidas läheb?");
        assert!(!result.tokens.is_empty());
        assert!(!result.words.is_empty());
        assert!(!result.sentences.is_empty());
    }

    #[test]
    fn test_estonian_text() {
        let pipeline = SegmentationPipeline::estonian();
        let text = "Eesti Vabariik on riik Põhja-Euroopas. Pealinn on Tallinn.";
        let result = pipeline.segment(text);
        assert!(!result.tokens.is_empty());
        assert!(result.sentences.len() >= 2, "Expected at least 2 sentences");
    }

    #[test]
    fn test_paragraph_detection() {
        let pipeline = SegmentationPipeline::estonian();
        let text = "Esimene lause.\n\nTeine lõik.";
        let result = pipeline.segment(text);
        assert!(result.paragraphs.len() >= 2, "Expected 2 paragraphs, got {}", result.paragraphs.len());
    }

    #[test]
    fn test_compound_date() {
        let pipeline = SegmentationPipeline::estonian();
        let text = "Kuupäev on 02.02.2010 ja see on hea.";
        let result = pipeline.segment(text);
        let has_date = result.compound_tokens.iter().any(|ct| {
            ct.pattern_type.iter().any(|t| t == "numeric_date")
        });
        assert!(has_date, "Expected a numeric_date compound token");
    }

    #[test]
    fn test_empty_text() {
        let pipeline = SegmentationPipeline::estonian();
        let result = pipeline.segment("");
        assert!(result.tokens.is_empty());
        assert!(result.words.is_empty());
        assert!(result.sentences.is_empty());
        assert!(result.paragraphs.is_empty());
    }
}
