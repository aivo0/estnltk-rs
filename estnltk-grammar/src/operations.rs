use std::collections::{HashMap, HashSet};

use crate::grammar::{match_seq_pattern, Grammar};

/// Generate all terminal phrases derivable from the grammar.
///
/// Top-down recursive expansion of grammar rules. SEQ(X) is expanded to
/// tuples of length 1..expand_seq.
///
/// Port of Python `phrase_list_generator()` from `grammar_operations.py`.
pub fn phrase_list_generator(
    grammar: &Grammar,
    depth_limit: Option<u32>,
    width_limit: Option<u32>,
    expand_seq: Option<u32>,
) -> Vec<Vec<String>> {
    let depth_lim = depth_limit.unwrap_or_else(|| match grammar.depth_limit() {
        crate::grammar::DepthLimit::Finite(n) => n,
        crate::grammar::DepthLimit::Unlimited => u32::MAX,
    });
    let width_lim = width_limit.unwrap_or_else(|| match grammar.width_limit() {
        crate::grammar::WidthLimit::Finite(n) => n,
        crate::grammar::WidthLimit::Unlimited => u32::MAX,
    });
    let exp_seq = expand_seq.unwrap_or(width_lim);

    let nonterminals = grammar.nonterminals();

    // Build ruledict: nonterminal → list of RHS tuples
    let mut ruledict: HashMap<&str, Vec<Vec<&str>>> = HashMap::new();
    for rule in grammar.rules() {
        ruledict
            .entry(&rule.lhs)
            .or_default()
            .push(rule.rhs.iter().map(|s| s.as_str()).collect());
    }
    // Add SEQ expansions
    for rule in grammar.rules() {
        for r in &rule.rhs {
            if let Some(inner) = match_seq_pattern(r) {
                if !ruledict.contains_key(r.as_str()) {
                    let expansions: Vec<Vec<&str>> = (1..=exp_seq as usize)
                        .map(|i| vec![inner; i])
                        .collect();
                    ruledict.insert(r, expansions);
                }
            }
        }
    }

    let mut yielded: HashSet<Vec<String>> = HashSet::new();
    let mut results = Vec::new();

    fn gen<'a>(
        phrase: Vec<&'a str>,
        depth: u32,
        width_lim: u32,
        nonterminals: &HashSet<String>,
        ruledict: &HashMap<&'a str, Vec<Vec<&'a str>>>,
        yielded: &mut HashSet<Vec<String>>,
        results: &mut Vec<Vec<String>>,
    ) {
        if phrase.len() > width_lim as usize {
            return;
        }

        // Find first nonterminal
        let mut nt_idx = None;
        for (i, &s) in phrase.iter().enumerate() {
            if nonterminals.contains(s) {
                nt_idx = Some(i);
                break;
            }
        }

        match nt_idx {
            None => {
                // All terminals
                let owned: Vec<String> = phrase.iter().map(|&s| s.to_string()).collect();
                if yielded.insert(owned.clone()) {
                    results.push(owned);
                }
            }
            Some(i) => {
                if depth == 0 {
                    return;
                }
                let nonterminal = phrase[i];
                if let Some(replacements) = ruledict.get(nonterminal) {
                    for replacement in replacements {
                        let mut new_phrase: Vec<&str> = Vec::with_capacity(
                            phrase.len() - 1 + replacement.len(),
                        );
                        new_phrase.extend_from_slice(&phrase[..i]);
                        new_phrase.extend_from_slice(replacement);
                        new_phrase.extend_from_slice(&phrase[i + 1..]);
                        gen(
                            new_phrase,
                            depth - 1,
                            width_lim,
                            nonterminals,
                            ruledict,
                            yielded,
                            results,
                        );
                    }
                }
            }
        }
    }

    let start: Vec<&str> = grammar.start_symbols().iter().map(|s| s.as_str()).collect();
    gen(
        start,
        depth_lim,
        width_lim,
        nonterminals,
        &ruledict,
        &mut yielded,
        &mut results,
    );

    results
}

/// Compute the n-gram fingerprint of a grammar.
///
/// Extracts all n-grams (up to size `n`) from all possible grammar phrases,
/// keeping a minimal set of n-gram collections using subset ordering.
///
/// Port of Python `ngram_fingerprint()` from `grammar_operations.py`.
pub fn ngram_fingerprint(
    n: usize,
    grammar: &Grammar,
    depth_limit: Option<u32>,
    width_limit: Option<u32>,
    expand_seq: Option<u32>,
) -> Vec<Vec<Vec<String>>> {
    let phrases = phrase_list_generator(grammar, depth_limit, width_limit, expand_seq);

    let mut ngrams_set: Vec<HashSet<Vec<String>>> = Vec::new();

    for phrase in &phrases {
        let m = n.min(phrase.len());
        let mut ngrams: HashSet<Vec<String>> = HashSet::new();
        for i in 0..=phrase.len().saturating_sub(m) {
            if i + m <= phrase.len() {
                ngrams.insert(phrase[i..i + m].to_vec());
            }
        }

        let mut add = true;
        let mut to_remove = Vec::new();
        for (idx, ng) in ngrams_set.iter().enumerate() {
            if ng.is_subset(&ngrams) {
                add = false;
                break;
            }
            if ngrams.is_subset(ng) {
                to_remove.push(idx);
            }
        }
        if add {
            // Remove supersets
            for &idx in to_remove.iter().rev() {
                ngrams_set.remove(idx);
            }
            ngrams_set.push(ngrams);
        }
    }

    // Sort for deterministic output
    let mut result: Vec<Vec<Vec<String>>> = ngrams_set
        .into_iter()
        .map(|ng_set| {
            let mut v: Vec<Vec<String>> = ng_set.into_iter().collect();
            v.sort();
            v
        })
        .collect();
    // Inner vecs are already sorted above; just sort outer by (len, content)
    result.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::{GrammarBuilder, Rule};

    #[test]
    fn test_phrase_list_generator() {
        let mut builder = GrammarBuilder::new()
            .start_symbols(vec!["S"]);
        builder.add_rule(Rule::new("S", "A").unwrap());
        builder.add_rule(Rule::new("S", "B").unwrap());
        builder.add_rule(Rule::new("S", "SEQ(B)").unwrap());
        builder.add_rule(Rule::new("B", "SEQ(C)").unwrap());
        builder.add_rule(Rule::new("A", "B F").unwrap());
        builder.add_rule(Rule::new("B", "G").unwrap());
        builder.add_rule(Rule::new("S", "K L M N").unwrap());
        let grammar = builder.build().unwrap();

        let phrases = phrase_list_generator(&grammar, None, None, Some(2));

        let expected: Vec<Vec<String>> = vec![
            vec!["C", "F"],
            vec!["C", "C", "F"],
            vec!["G", "F"],
            vec!["C"],
            vec!["C", "C"],
            vec!["G"],
            vec!["C", "C", "C"],
            vec!["C", "G"],
            vec!["C", "C", "C", "C"],
            vec!["C", "C", "G"],
            vec!["G", "C"],
            vec!["G", "C", "C"],
            vec!["G", "G"],
            vec!["K", "L", "M", "N"],
        ]
        .into_iter()
        .map(|v| v.into_iter().map(|s| s.to_string()).collect())
        .collect();

        assert_eq!(phrases, expected);
    }

    #[test]
    fn test_ngram_fingerprint() {
        let mut builder = GrammarBuilder::new()
            .start_symbols(vec!["S"]);
        builder.add_rule(Rule::new("S", "A").unwrap());
        builder.add_rule(Rule::new("S", "B").unwrap());
        builder.add_rule(Rule::new("S", "SEQ(B)").unwrap());
        builder.add_rule(Rule::new("B", "SEQ(C)").unwrap());
        builder.add_rule(Rule::new("A", "B F").unwrap());
        builder.add_rule(Rule::new("B", "G").unwrap());
        builder.add_rule(Rule::new("S", "K L M N").unwrap());
        let grammar = builder.build().unwrap();

        let result = ngram_fingerprint(2, &grammar, None, None, Some(2));

        let expected: Vec<Vec<Vec<String>>> = vec![
            vec![vec!["C"]],
            vec![vec!["C", "C"]],
            vec![vec!["C", "F"]],
            vec![vec!["C", "G"]],
            vec![vec!["G"]],
            vec![vec!["G", "C"]],
            vec![vec!["G", "F"]],
            vec![vec!["G", "G"]],
            vec![vec!["K", "L"], vec!["L", "M"], vec!["M", "N"]],
        ]
        .into_iter()
        .map(|ng| {
            ng.into_iter()
                .map(|v| v.into_iter().map(|s| s.to_string()).collect())
                .collect()
        })
        .collect();

        assert_eq!(result, expected);
    }
}
