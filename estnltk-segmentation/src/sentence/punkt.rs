use super::punkt_params::{PunktParameters, ORTHO_BEG_UC, ORTHO_LC, ORTHO_MID_UC};

/// A token annotated by the Punkt algorithm.
#[derive(Debug, Clone)]
struct PunktToken {
    /// Original text of the token
    text: String,
    /// Lowercased type (with trailing period removed if present)
    type_str: String,
    /// Whether the token ends with a period
    period_final: bool,
    /// Whether this token marks a sentence break (annotation result)
    sentbreak: bool,
    /// Whether this is an abbreviation
    abbr: bool,
    /// Whether this is an ellipsis
    ellipsis: bool,
    /// Whether this is an initial (e.g., "A.")
    is_initial: bool,
    /// First character is uppercase
    first_upper: bool,
    /// First character is lowercase
    first_lower: bool,
}

impl PunktToken {
    fn new(text: &str) -> Self {
        let period_final = text.ends_with('.');
        let type_str = if period_final {
            text[..text.len() - 1].to_lowercase()
        } else {
            text.to_lowercase()
        };

        let first_char = text.chars().next();
        let first_upper = first_char.map_or(false, |c| c.is_uppercase());
        let first_lower = first_char.map_or(false, |c| c.is_lowercase());

        // Check if it's an initial: single letter followed by period
        let is_initial = period_final && {
            let without_period = &text[..text.len() - 1];
            let char_count = without_period.chars().count();
            char_count == 1 && without_period.chars().next().map_or(false, |c| c.is_alphabetic())
        };

        PunktToken {
            text: text.to_string(),
            type_str,
            period_final,
            sentbreak: false,
            abbr: false,
            ellipsis: false,
            is_initial,
            first_upper,
            first_lower,
        }
    }

    /// Get the type with period included if period_final
    fn type_with_period(&self) -> String {
        if self.period_final {
            format!("{}.", self.type_str)
        } else {
            self.type_str.clone()
        }
    }

    /// Get type without period
    fn type_no_period(&self) -> &str {
        &self.type_str
    }

    /// Get type without sentence-break annotation
    fn type_no_sentperiod(&self) -> String {
        if self.sentbreak {
            self.type_str.clone()
        } else {
            self.type_with_period()
        }
    }
}

/// Run the Punkt sentence-from-tokens algorithm.
///
/// Given a list of word texts (in order), returns the indices of the last
/// word in each sentence. This is the `sentences_from_tokens` path from NLTK.
pub fn sentences_from_tokens(word_texts: &[&str], params: &PunktParameters) -> Vec<usize> {
    if word_texts.is_empty() {
        return Vec::new();
    }

    // Create tokens
    let mut tokens: Vec<PunktToken> = word_texts.iter().map(|t| PunktToken::new(t)).collect();

    // First pass: annotate
    annotate_first_pass(&mut tokens, params);

    // Second pass: pairwise annotation
    annotate_second_pass(&mut tokens, params);

    // Extract sentence boundaries
    let mut sentence_breaks = Vec::new();
    let mut in_sentence = false;

    for (i, token) in tokens.iter().enumerate() {
        if !in_sentence {
            in_sentence = true;
        }

        if token.sentbreak {
            sentence_breaks.push(i);
            in_sentence = false;
        }
    }

    // The last token always ends the last sentence
    if in_sentence && !tokens.is_empty() {
        let last_idx = tokens.len() - 1;
        if sentence_breaks.last() != Some(&last_idx) {
            sentence_breaks.push(last_idx);
        }
    }

    // Convert to sentence ranges: each entry is the index of the last word in each sentence
    // For sentences_from_tokens, we group tokens between sentbreaks
    let mut sentences: Vec<Vec<usize>> = Vec::new();
    let mut current_sentence: Vec<usize> = Vec::new();

    for (i, token) in tokens.iter().enumerate() {
        current_sentence.push(i);
        if token.sentbreak || i == tokens.len() - 1 {
            if !current_sentence.is_empty() {
                sentences.push(current_sentence.clone());
            }
            current_sentence.clear();
        }
    }

    // Return the index of last token in each sentence group
    sentences.iter()
        .map(|s| *s.last().unwrap())
        .collect()
}

/// First pass: mark sentence-ending punctuation, detect ellipsis, check abbreviations.
fn annotate_first_pass(tokens: &mut [PunktToken], params: &PunktParameters) {
    for token in tokens.iter_mut() {
        // Check for ellipsis: 2+ consecutive periods
        if token.text.chars().all(|c| c == '.') && token.text.chars().count() >= 2 {
            token.ellipsis = true;
            continue;
        }

        // Mark sentence-ending punctuation
        if token.text == "." || token.text == "?" || token.text == "!" {
            token.sentbreak = true;
            continue;
        }

        // Check for period-final tokens
        if token.period_final {
            // Check if it's a known abbreviation
            let type_no_period = token.type_no_period();
            if params.abbrev_types.contains(type_no_period) {
                token.abbr = true;
            } else if token.is_initial {
                // Single letter + period is likely an initial/abbreviation
                token.abbr = true;
            } else {
                // Period at end of non-abbreviation = sentence break
                token.sentbreak = true;
            }
        }
    }
}

/// Second pass: pairwise checks for collocations, orthographic context, and sentence starters.
fn annotate_second_pass(tokens: &mut [PunktToken], params: &PunktParameters) {
    if tokens.len() < 2 {
        return;
    }

    // We need to work with pairs (tok1, tok2) where tok1 might have been marked as sentbreak
    // and we check if that sentbreak should be undone.
    // We iterate indices and modify in-place.
    for i in 0..tokens.len() - 1 {
        // Only process tokens that are marked as sentbreak or abbreviation
        if !tokens[i].sentbreak && !tokens[i].abbr {
            continue;
        }

        let tok1_type = tokens[i].type_no_period().to_string();
        let tok2_type = tokens[i + 1].type_no_sentperiod();
        let tok2_first_upper = tokens[i + 1].first_upper;
        let _tok2_first_lower = tokens[i + 1].first_lower;
        let tok1_is_initial = tokens[i].is_initial;
        let tok1_sentbreak = tokens[i].sentbreak;

        // Collocation heuristic:
        // If (tok1_type, tok2_type) is a known collocation, undo sentbreak
        if tok1_sentbreak {
            let pair = (tok1_type.clone(), tok2_type.clone());
            if params.collocations.contains(&pair) {
                tokens[i].sentbreak = false;
                tokens[i].abbr = true;
                continue;
            }
        }

        // Orthographic heuristic for abbreviations:
        // If tok1 is an abbreviation, check if tok2's orthographic context
        // suggests it's NOT the start of a new sentence
        if (tokens[i].abbr || tok1_is_initial) && tok2_first_upper {
            let ortho = params.ortho_context.get(&tok2_type).copied().unwrap_or(0);

            // If tok2 is seen in lowercase contexts, it's not a reliable sentence start
            if ortho & ORTHO_LC != 0 && ortho & ORTHO_MID_UC == 0 {
                tokens[i].sentbreak = false;
                tokens[i].abbr = true;
                continue;
            }
        }

        // Sentence-starter heuristic:
        // If tok1 is an abbreviation (not sentbreak) and tok2 is uppercase,
        // check if tok2 is a frequent sentence starter
        if tokens[i].abbr && !tokens[i].sentbreak && tok2_first_upper {
            let ortho = params.ortho_context.get(&tok2_type).copied().unwrap_or(0);

            // If tok2 is always seen at beginning of sentence in uppercase,
            // and never in middle of sentence in uppercase, add sentbreak
            if ortho & ORTHO_BEG_UC != 0 && ortho & ORTHO_MID_UC == 0 {
                if params.sent_starters.contains(&tok2_type) {
                    tokens[i].sentbreak = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_sentence_split() {
        let params = PunktParameters::estonian();
        let words = vec!["Tere", "maailm", ".", "Kuidas", "läheb", "?"];
        let breaks = sentences_from_tokens(&words, params);
        // Should have 2 sentences
        assert_eq!(breaks.len(), 2);
    }

    #[test]
    fn test_single_sentence() {
        let params = PunktParameters::estonian();
        let words = vec!["Tere", "maailm"];
        let breaks = sentences_from_tokens(&words, params);
        assert_eq!(breaks.len(), 1);
        assert_eq!(breaks[0], 1); // last word index
    }

    #[test]
    fn test_empty_input() {
        let params = PunktParameters::estonian();
        let words: Vec<&str> = vec![];
        let breaks = sentences_from_tokens(&words, params);
        assert!(breaks.is_empty());
    }
}
