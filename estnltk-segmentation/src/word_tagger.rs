use std::collections::HashSet;

use estnltk_core::MatchSpan;

use crate::compound_token::CompoundToken;

/// A word with its span and optional normalized form.
#[derive(Debug, Clone)]
pub struct Word {
    pub span: MatchSpan,
    pub normalized_form: Option<String>,
}

/// Assemble words from tokens and compound tokens.
///
/// Compound tokens override individual tokens: all token spans that fall within
/// a compound token are replaced by the compound token's bounding span.
pub fn assemble_words(
    tokens: &[MatchSpan],
    compound_tokens: &[CompoundToken],
) -> Vec<Word> {
    // Collect all elementary token spans covered by compound tokens
    let mut covered: HashSet<MatchSpan> = HashSet::new();
    let mut words = Vec::new();

    // First add compound tokens as words
    for ct in compound_tokens {
        words.push(Word {
            span: ct.span,
            normalized_form: ct.normalized.clone(),
        });
        for &token_span in &ct.token_spans {
            covered.insert(token_span);
        }
    }

    // Then add tokens not covered by compound tokens
    for &token in tokens {
        if !covered.contains(&token) {
            words.push(Word {
                span: token,
                normalized_form: None,
            });
        }
    }

    // Sort by span start position
    words.sort_by(|a, b| a.span.start.cmp(&b.span.start).then(a.span.end.cmp(&b.span.end)));

    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_compounds() {
        let tokens = vec![
            MatchSpan::new(0, 5),
            MatchSpan::new(6, 11),
        ];
        let words = assemble_words(&tokens, &[]);
        assert_eq!(words.len(), 2);
        assert!(words[0].normalized_form.is_none());
    }

    #[test]
    fn test_with_compound() {
        let tokens = vec![
            MatchSpan::new(0, 2),  // "02"
            MatchSpan::new(2, 3),  // "."
            MatchSpan::new(3, 5),  // "02"
            MatchSpan::new(5, 6),  // "."
            MatchSpan::new(6, 10), // "2010"
            MatchSpan::new(11, 16), // "hello"
        ];
        let compound_tokens = vec![CompoundToken {
            span: MatchSpan::new(0, 10),
            token_spans: vec![
                MatchSpan::new(0, 2),
                MatchSpan::new(2, 3),
                MatchSpan::new(3, 5),
                MatchSpan::new(5, 6),
                MatchSpan::new(6, 10),
            ],
            pattern_type: vec!["numeric_date".to_string()],
            normalized: Some("02.02.2010".to_string()),
        }];
        let words = assemble_words(&tokens, &compound_tokens);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].span, MatchSpan::new(0, 10));
        assert_eq!(words[0].normalized_form, Some("02.02.2010".to_string()));
        assert_eq!(words[1].span, MatchSpan::new(11, 16));
    }
}
