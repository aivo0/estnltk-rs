use std::collections::HashMap;

use aho_corasick::AhoCorasick;

use crate::byte_char::byte_to_char_map;
use crate::conflict::{
    conflict_priority_resolver, keep_maximal_matches, keep_minimal_matches,
};
use crate::types::{
    has_missing_attributes, normalize_annotation, Annotation, AnnotationValue, ConflictStrategy,
    MatchSpan, TagResult, TaggedSpan, TaggerConfig,
};

/// A substring extraction rule — pattern string with static attributes.
/// Unlike `ExtractionRule`, no compiled regex; the automaton handles matching.
#[derive(Debug, Clone)]
pub struct SubstringRule {
    pub pattern_str: String,
    pub attributes: HashMap<String, AnnotationValue>,
    pub group: u32,
    pub priority: i32,
}

/// The core substring tagger — Rust equivalent of EstNLTK's `SubstringTagger`.
///
/// Uses a single Aho-Corasick automaton built from all unique patterns.
/// Supports token separator boundary checking and all conflict resolution strategies.
pub struct SubstringTagger {
    automaton: AhoCorasick,
    rules: Vec<SubstringRule>,
    /// AC pattern_id → list of rule indices sharing that pattern.
    pattern_to_rules: Vec<Vec<usize>>,
    token_separators: Vec<char>,
    pub config: TaggerConfig,
}

impl SubstringTagger {
    /// Create a new SubstringTagger from rules and configuration.
    ///
    /// Builds an Aho-Corasick automaton from unique patterns.
    /// If `config.lowercase_text` is true, patterns are lowercased before deduplication.
    pub fn new(
        rules: Vec<SubstringRule>,
        token_separators: &str,
        config: TaggerConfig,
    ) -> Result<Self, String> {
        // Deduplicate patterns, mapping each unique pattern to its rule indices.
        let mut pattern_map: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, rule) in rules.iter().enumerate() {
            let key = if config.lowercase_text {
                rule.pattern_str.to_lowercase()
            } else {
                rule.pattern_str.clone()
            };
            pattern_map.entry(key).or_default().push(i);
        }

        // Collect unique patterns in deterministic order (sorted for reproducibility).
        let mut unique_patterns: Vec<String> = pattern_map.keys().cloned().collect();
        unique_patterns.sort();

        // Build pattern_to_rules parallel to unique_patterns.
        let pattern_to_rules: Vec<Vec<usize>> = unique_patterns
            .iter()
            .map(|p| pattern_map[p].clone())
            .collect();

        // Build the Aho-Corasick automaton.
        let automaton = AhoCorasick::new(&unique_patterns)
            .map_err(|e| format!("Aho-Corasick build error: {}", e))?;

        let separators: Vec<char> = token_separators.chars().collect();

        Ok(Self {
            automaton,
            rules,
            pattern_to_rules,
            token_separators: separators,
            config,
        })
    }

    /// Run the full tagging pipeline on a text string.
    pub fn tag(&self, text: &str) -> TagResult {
        let raw_text = if self.config.lowercase_text {
            text.to_lowercase()
        } else {
            text.to_string()
        };

        // Step 1: Extract unique matches (one per AC match, indexed by pattern_id).
        let mut all_matches = self.extract_matches(&raw_text);

        // Step 2: Sort canonically by (start, end).
        all_matches.sort_by_key(|&(span, _)| (span.start, span.end));

        // Step 3: Apply conflict resolution on unique spans.
        let resolved = self.resolve_conflicts(&all_matches);

        // Step 4: Build TagResult, fanning out to all rules per pattern.
        self.build_result(&resolved)
    }

    /// A match entry: (span, pattern_id). One per AC match, not fanned out to rules.
    /// Conflict resolution operates on these unique entries.
    /// Fan-out to individual rules happens in `build_result`.

    /// Extract raw matches using Aho-Corasick, converting byte→char offsets.
    ///
    /// Returns one entry per AC match with pattern_id (not rule index).
    /// Uses `find_overlapping_iter` to match Python's `ahocorasick.iter()` behavior.
    fn extract_matches(&self, text: &str) -> Vec<(MatchSpan, usize)> {
        let b2c = byte_to_char_map(text);
        let mut matches = Vec::new();

        for mat in self.automaton.find_overlapping_iter(text) {
            let byte_start = mat.start();
            let byte_end = mat.end();
            let char_start = b2c[byte_start];
            let char_end = b2c[byte_end];

            // Skip zero-length matches.
            if char_start == char_end {
                continue;
            }

            // Token separator boundary check.
            if !self.token_separators.is_empty() {
                // Check that the character before the match is a separator (or match starts at text start).
                if byte_start > 0 {
                    let prev_char = text[..byte_start].chars().next_back().unwrap();
                    if !self.token_separators.contains(&prev_char) {
                        continue;
                    }
                }
                // Check that the character after the match is a separator (or match ends at text end).
                if byte_end < text.len() {
                    let next_char = text[byte_end..].chars().next().unwrap();
                    if !self.token_separators.contains(&next_char) {
                        continue;
                    }
                }
            }

            let pattern_id = mat.pattern().as_usize();
            matches.push((MatchSpan::new(char_start, char_end), pattern_id));
        }

        matches
    }

    /// Apply the configured conflict resolution strategy.
    /// Operates on (span, pattern_id) entries — one per unique AC match.
    fn resolve_conflicts(&self, sorted: &[(MatchSpan, usize)]) -> Vec<(MatchSpan, usize)> {
        match self.config.conflict_strategy {
            ConflictStrategy::KeepAll => sorted.to_vec(),
            ConflictStrategy::KeepMaximal => keep_maximal_matches(sorted),
            ConflictStrategy::KeepMinimal => keep_minimal_matches(sorted),
            ConflictStrategy::KeepAllExceptPriority => {
                let (groups, priorities) = self.extract_group_priority(sorted);
                conflict_priority_resolver(sorted, &groups, &priorities)
            }
            ConflictStrategy::KeepMaximalExceptPriority => {
                let (groups, priorities) = self.extract_group_priority(sorted);
                let after_priority = conflict_priority_resolver(sorted, &groups, &priorities);
                keep_maximal_matches(&after_priority)
            }
            ConflictStrategy::KeepMinimalExceptPriority => {
                let (groups, priorities) = self.extract_group_priority(sorted);
                let after_priority = conflict_priority_resolver(sorted, &groups, &priorities);
                keep_minimal_matches(&after_priority)
            }
        }
    }

    /// Extract group and priority arrays for the priority resolver.
    /// Uses the first rule's group/priority for each pattern.
    fn extract_group_priority(&self, entries: &[(MatchSpan, usize)]) -> (Vec<i32>, Vec<i32>) {
        let groups: Vec<i32> = entries
            .iter()
            .map(|(_, pattern_id)| {
                let first_rule = self.pattern_to_rules[*pattern_id][0];
                self.rules[first_rule].group as i32
            })
            .collect();
        let priorities: Vec<i32> = entries
            .iter()
            .map(|(_, pattern_id)| {
                let first_rule = self.pattern_to_rules[*pattern_id][0];
                self.rules[first_rule].priority
            })
            .collect();
        (groups, priorities)
    }

    /// Build the final TagResult from resolved matches.
    /// Fans out each (span, pattern_id) to all rules sharing that pattern.
    fn build_result(&self, resolved: &[(MatchSpan, usize)]) -> TagResult {
        let mut spans: Vec<TaggedSpan> = Vec::new();

        for &(match_span, pattern_id) in resolved {
            for &rule_idx in &self.pattern_to_rules[pattern_id] {
                let rule = &self.rules[rule_idx];
                let mut annotation = Annotation::new();

                // Copy static attributes from rule.
                for (k, v) in &rule.attributes {
                    annotation.0.insert(k.clone(), v.clone());
                }

                // Optionally add group/priority/pattern attributes.
                if let Some(ref attr_name) = self.config.group_attribute {
                    annotation
                        .0
                        .insert(attr_name.clone(), AnnotationValue::Int(rule.group as i64));
                }
                if let Some(ref attr_name) = self.config.priority_attribute {
                    annotation
                        .0
                        .insert(attr_name.clone(), AnnotationValue::Int(rule.priority as i64));
                }
                if let Some(ref attr_name) = self.config.pattern_attribute {
                    annotation.0.insert(
                        attr_name.clone(),
                        AnnotationValue::Str(rule.pattern_str.clone()),
                    );
                }

                // Normalize: fill missing output_attributes with Null.
                normalize_annotation(&mut annotation, &self.config.output_attributes);

                // Merge into existing span or create new one.
                if let Some(last) = spans.last_mut() {
                    if last.span == match_span {
                        last.annotations.push(annotation);
                        continue;
                    }
                }
                spans.push(TaggedSpan {
                    span: match_span,
                    annotations: vec![annotation],
                });
            }
        }

        TagResult {
            name: self.config.output_layer.clone(),
            attributes: self.config.output_attributes.clone(),
            ambiguous: true,
            spans,
        }
    }

    /// Check if rules have inconsistent attribute sets.
    ///
    /// Returns `true` if some rules don't define the same set of attributes.
    /// Maps to EstNLTK's `AmbiguousRuleset.missing_attributes` property.
    pub fn missing_attributes(&self) -> bool {
        let attrs: Vec<&HashMap<String, AnnotationValue>> =
            self.rules.iter().map(|r| &r.attributes).collect();
        has_missing_attributes(&attrs)
    }
}

/// Convenience: build a SubstringRule from components.
pub fn make_substring_rule(
    pattern: &str,
    attributes: HashMap<String, AnnotationValue>,
    group: u32,
    priority: i32,
) -> SubstringRule {
    SubstringRule {
        pattern_str: pattern.to_string(),
        attributes,
        group,
        priority,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> TaggerConfig {
        TaggerConfig {
            output_layer: "test".to_string(),
            output_attributes: vec![],
            conflict_strategy: ConflictStrategy::KeepMaximal,
            lowercase_text: false,
            group_attribute: None,
            priority_attribute: None,
            pattern_attribute: None,
        }
    }

    #[test]
    fn test_simple_match() {
        let rules = vec![
            make_substring_rule("first", HashMap::new(), 0, 0),
            make_substring_rule("firs", HashMap::new(), 0, 0),
            make_substring_rule("irst", HashMap::new(), 0, 0),
            make_substring_rule("last", HashMap::new(), 0, 0),
        ];
        let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
        let result = tagger.tag("first second last");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
        assert_eq!(result.spans[1].span, MatchSpan::new(13, 17));
    }

    #[test]
    fn test_ignore_case() {
        let rules = vec![
            make_substring_rule("First", HashMap::new(), 0, 0),
            make_substring_rule("firs", HashMap::new(), 0, 0),
            make_substring_rule("irst", HashMap::new(), 0, 0),
            make_substring_rule("LAST", HashMap::new(), 0, 0),
        ];
        let mut cfg = default_config();
        cfg.lowercase_text = true;
        let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
        let result = tagger.tag("first second last");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
        assert_eq!(result.spans[1].span, MatchSpan::new(13, 17));
    }

    #[test]
    fn test_separator_pipe() {
        let rules = vec![make_substring_rule("match", HashMap::new(), 0, 0)];
        let tagger = SubstringTagger::new(rules, "|", default_config()).unwrap();
        let result = tagger.tag("match|match| match| match| match |match");
        // Valid: "match" at 0..5 (start of text), "match" at 6..11 (|match|), "match" at 34..39 (|match at end)
        assert_eq!(result.spans.len(), 3);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
        assert_eq!(result.spans[1].span, MatchSpan::new(6, 11));
        assert_eq!(result.spans[2].span, MatchSpan::new(34, 39));
    }

    #[test]
    fn test_separator_multiple() {
        let rules = vec![make_substring_rule("match", HashMap::new(), 0, 0)];
        let tagger = SubstringTagger::new(rules, " ,:", default_config()).unwrap();
        let result = tagger.tag("match match, :match, match");
        assert_eq!(result.spans.len(), 4);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
        assert_eq!(result.spans[1].span, MatchSpan::new(6, 11));
        assert_eq!(result.spans[2].span, MatchSpan::new(14, 19));
        assert_eq!(result.spans[3].span, MatchSpan::new(21, 26));
    }

    #[test]
    fn test_annotations() {
        let mut a1 = HashMap::new();
        a1.insert("a".to_string(), AnnotationValue::Int(1));
        a1.insert("b".to_string(), AnnotationValue::Int(1));
        let mut a2 = HashMap::new();
        a2.insert("a".to_string(), AnnotationValue::Int(3));
        a2.insert("b".to_string(), AnnotationValue::Int(2));
        let mut a3 = HashMap::new();
        a3.insert("a".to_string(), AnnotationValue::Int(3));
        a3.insert("b".to_string(), AnnotationValue::Int(5));

        let rules = vec![
            make_substring_rule("first", a1, 0, 0),
            make_substring_rule("second", a2, 0, 0),
            make_substring_rule("last", a3, 0, 0),
        ];
        let mut cfg = default_config();
        cfg.output_attributes = vec!["a".to_string(), "b".to_string()];
        let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
        let result = tagger.tag("first second last");
        assert_eq!(result.spans.len(), 3);
        assert_eq!(
            result.spans[0].annotations[0].0.get("a"),
            Some(&AnnotationValue::Int(1))
        );
        assert_eq!(
            result.spans[1].annotations[0].0.get("b"),
            Some(&AnnotationValue::Int(2))
        );
        assert_eq!(
            result.spans[2].annotations[0].0.get("a"),
            Some(&AnnotationValue::Int(3))
        );
    }

    #[test]
    fn test_keep_all() {
        let rules = vec![
            make_substring_rule("abcd", HashMap::new(), 0, 0),
            make_substring_rule("abc", HashMap::new(), 0, 0),
            make_substring_rule("bc", HashMap::new(), 0, 0),
            make_substring_rule("bcd", HashMap::new(), 0, 0),
            make_substring_rule("bcde", HashMap::new(), 0, 0),
            make_substring_rule("f", HashMap::new(), 0, 0),
            make_substring_rule("ef", HashMap::new(), 0, 0),
        ];
        let mut cfg = default_config();
        cfg.conflict_strategy = ConflictStrategy::KeepAll;
        let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
        let result = tagger.tag("abcdea--efg");
        assert_eq!(result.spans.len(), 7);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 3)); // abc
        assert_eq!(result.spans[1].span, MatchSpan::new(0, 4)); // abcd
        assert_eq!(result.spans[2].span, MatchSpan::new(1, 3)); // bc
        assert_eq!(result.spans[3].span, MatchSpan::new(1, 4)); // bcd
        assert_eq!(result.spans[4].span, MatchSpan::new(1, 5)); // bcde
        assert_eq!(result.spans[5].span, MatchSpan::new(8, 10)); // ef
        assert_eq!(result.spans[6].span, MatchSpan::new(9, 10)); // f
    }

    #[test]
    fn test_keep_maximal() {
        let rules = vec![
            make_substring_rule("abcd", HashMap::new(), 0, 0),
            make_substring_rule("abc", HashMap::new(), 0, 0),
            make_substring_rule("bc", HashMap::new(), 0, 0),
            make_substring_rule("bcd", HashMap::new(), 0, 0),
            make_substring_rule("bcde", HashMap::new(), 0, 0),
            make_substring_rule("f", HashMap::new(), 0, 0),
            make_substring_rule("ef", HashMap::new(), 0, 0),
        ];
        let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
        let result = tagger.tag("abcdea--efg");
        assert_eq!(result.spans.len(), 3);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 4)); // abcd
        assert_eq!(result.spans[1].span, MatchSpan::new(1, 5)); // bcde
        assert_eq!(result.spans[2].span, MatchSpan::new(8, 10)); // ef
    }

    #[test]
    fn test_keep_minimal() {
        let rules = vec![
            make_substring_rule("abcd", HashMap::new(), 0, 0),
            make_substring_rule("abc", HashMap::new(), 0, 0),
            make_substring_rule("bc", HashMap::new(), 0, 0),
            make_substring_rule("bcd", HashMap::new(), 0, 0),
            make_substring_rule("bcde", HashMap::new(), 0, 0),
            make_substring_rule("f", HashMap::new(), 0, 0),
            make_substring_rule("ef", HashMap::new(), 0, 0),
        ];
        let mut cfg = default_config();
        cfg.conflict_strategy = ConflictStrategy::KeepMinimal;
        let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
        let result = tagger.tag("abcdea--efg");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(1, 3)); // bc
        assert_eq!(result.spans[1].span, MatchSpan::new(9, 10)); // f
    }

    #[test]
    fn test_estonian_multibyte() {
        // "öö" in "Tüüpiline öökülma näide"
        let rules = vec![make_substring_rule("öö", HashMap::new(), 0, 0)];
        let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
        let result = tagger.tag("Tüüpiline öökülma näide");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(10, 12));
    }

    #[test]
    fn test_no_match() {
        let rules = vec![make_substring_rule("xyz", HashMap::new(), 0, 0)];
        let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
        let result = tagger.tag("hello world");
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_empty_text() {
        let rules = vec![make_substring_rule("hello", HashMap::new(), 0, 0)];
        let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
        let result = tagger.tag("");
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_ambiguous_rules() {
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("capital".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("name".to_string()));

        let rules = vec![
            make_substring_rule("Washington", a1, 0, 0),
            make_substring_rule("Washington", a2, 0, 0),
        ];
        let mut cfg = default_config();
        cfg.output_attributes = vec!["type".to_string()];
        let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
        let result = tagger.tag("Washington");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].annotations.len(), 2);
    }

    #[test]
    fn test_missing_attributes_false_consistent() {
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("x".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("y".to_string()));
        let rules = vec![
            make_substring_rule("hello", a1, 0, 0),
            make_substring_rule("world", a2, 0, 0),
        ];
        let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
        assert!(!tagger.missing_attributes());
    }

    #[test]
    fn test_missing_attributes_true_inconsistent() {
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("x".to_string()));
        a1.insert("color".to_string(), AnnotationValue::Str("red".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("y".to_string()));
        let rules = vec![
            make_substring_rule("hello", a1, 0, 0),
            make_substring_rule("world", a2, 0, 0),
        ];
        let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
        assert!(tagger.missing_attributes());
    }

    #[test]
    fn test_normalize_annotations_fills_null() {
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("greeting".to_string()));
        a1.insert("score".to_string(), AnnotationValue::Int(10));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("noun".to_string()));

        let rules = vec![
            make_substring_rule("hello", a1, 0, 0),
            make_substring_rule("world", a2, 0, 0),
        ];
        let mut cfg = default_config();
        cfg.output_attributes = vec!["type".to_string(), "score".to_string()];
        let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
        let result = tagger.tag("hello world");

        assert_eq!(result.spans.len(), 2);
        // First span: has both attributes
        assert_eq!(
            result.spans[0].annotations[0].0.get("score"),
            Some(&AnnotationValue::Int(10))
        );
        // Second span: score should be Null
        assert_eq!(
            result.spans[1].annotations[0].0.get("type"),
            Some(&AnnotationValue::Str("noun".to_string()))
        );
        assert_eq!(
            result.spans[1].annotations[0].0.get("score"),
            Some(&AnnotationValue::Null)
        );
    }

    #[test]
    fn test_pattern_attribute() {
        let rules = vec![
            make_substring_rule("hello", HashMap::new(), 0, 0),
            make_substring_rule("world", HashMap::new(), 0, 0),
        ];
        let mut cfg = default_config();
        cfg.pattern_attribute = Some("_pattern".to_string());
        let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
        let result = tagger.tag("hello world");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(
            result.spans[0].annotations[0].0.get("_pattern"),
            Some(&AnnotationValue::Str("hello".to_string()))
        );
        assert_eq!(
            result.spans[1].annotations[0].0.get("_pattern"),
            Some(&AnnotationValue::Str("world".to_string()))
        );
    }
}
