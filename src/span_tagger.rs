use std::collections::HashMap;

use crate::conflict::{
    conflict_priority_resolver, keep_maximal_matches, keep_minimal_matches, MatchEntry,
};
use crate::types::{
    check_unique_patterns, has_missing_attributes, normalize_annotation, Annotation,
    AnnotationValue, ConflictStrategy, TagResult, TaggedSpan,
};

/// A rule for the SpanTagger — maps a pattern string to static attributes.
///
/// Unlike `ExtractionRule` (used by RegexTagger), this carries no compiled regex
/// because matching is exact string comparison against annotation attribute values.
#[derive(Debug, Clone)]
pub struct SpanRule {
    pub pattern_str: String,
    pub attributes: HashMap<String, AnnotationValue>,
    pub group: u32,
    pub priority: i32,
}

/// Create a SpanRule from components.
pub fn make_span_rule(
    pattern: &str,
    attributes: HashMap<String, AnnotationValue>,
    group: u32,
    priority: i32,
) -> SpanRule {
    SpanRule {
        pattern_str: pattern.to_string(),
        attributes,
        group,
        priority,
    }
}

/// The SpanTagger — Rust equivalent of EstNLTK's `SpanTagger`.
///
/// Matches attribute values from an input layer against a ruleset of exact
/// string patterns.  For each matching annotation, copies the rule's static
/// attributes into a new output layer annotation.
///
/// Input is a `TagResult` (output of RegexTagger, SubstringTagger, or another
/// SpanTagger), rather than EstNLTK's `Layer` object.
#[derive(Debug)]
pub struct SpanTagger {
    pub rules: Vec<SpanRule>,
    /// Maps pattern string → list of rule indices for O(1) lookup.
    ruleset_map: HashMap<String, Vec<usize>>,
    pub config: SpanTaggerConfig,
}

/// Configuration for the SpanTagger.
#[derive(Debug)]
pub struct SpanTaggerConfig {
    pub output_layer: String,
    pub input_attribute: String,
    pub output_attributes: Vec<String>,
    pub conflict_strategy: ConflictStrategy,
    pub ignore_case: bool,
    pub group_attribute: Option<String>,
    pub priority_attribute: Option<String>,
    pub pattern_attribute: Option<String>,
    pub ambiguous_output_layer: bool,
    pub unique_patterns: bool,
}

impl SpanTagger {
    /// Create a new SpanTagger, validating configuration.
    pub fn new(rules: Vec<SpanRule>, config: SpanTaggerConfig) -> Result<Self, String> {
        // Enforce unique patterns if configured.
        if config.unique_patterns {
            let patterns: Vec<&str> = rules.iter().map(|r| r.pattern_str.as_str()).collect();
            check_unique_patterns(&patterns, config.ignore_case)?;
        }

        // Build the pattern → rule-indices lookup map.
        let mut ruleset_map: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, rule) in rules.iter().enumerate() {
            let key = if config.ignore_case {
                rule.pattern_str.to_lowercase()
            } else {
                rule.pattern_str.clone()
            };
            ruleset_map.entry(key).or_default().push(i);
        }

        Ok(Self {
            rules,
            ruleset_map,
            config,
        })
    }

    /// Tag an input layer by matching attribute values against the ruleset.
    ///
    /// For each span in the input `TagResult`, inspects every annotation's
    /// `input_attribute` value.  When the value (exact string match, optionally
    /// case-insensitive) matches a pattern in the ruleset, new annotations are
    /// created in the output with the rule's static attributes.
    pub fn tag(&self, input: &TagResult) -> TagResult {
        // Step 1: Extract matches — (span, rule_index) pairs.
        let mut all_matches = self.extract_matches(input);

        // Step 2: Sort canonically by (start, end).
        all_matches.sort_by_key(|&(span, _)| (span.start, span.end));

        // Step 3: Apply conflict resolution.
        let resolved = self.resolve_conflicts(&all_matches);

        // Step 4: Build TagResult.
        self.build_result(&resolved)
    }

    /// Extract matches by scanning input annotations.
    fn extract_matches(&self, input: &TagResult) -> Vec<MatchEntry> {
        let mut matches = Vec::new();

        for tagged_span in &input.spans {
            for annotation in &tagged_span.annotations {
                // Get the attribute value to match.
                // For Str values, borrow directly to avoid cloning in the
                // common case-sensitive path. Non-string values need a
                // temporary String from Display.
                let owned_tmp;
                let value_str: &str = match annotation.0.get(&self.config.input_attribute) {
                    Some(AnnotationValue::Str(s)) => s.as_str(),
                    Some(AnnotationValue::Int(i)) => {
                        owned_tmp = i.to_string();
                        &owned_tmp
                    }
                    Some(AnnotationValue::Float(f)) => {
                        owned_tmp = f.to_string();
                        &owned_tmp
                    }
                    Some(AnnotationValue::Bool(b)) => {
                        owned_tmp = b.to_string();
                        &owned_tmp
                    }
                    Some(AnnotationValue::Null) | Some(AnnotationValue::List(_)) | None => continue,
                };

                // Look up in ruleset map. Case-insensitive requires an
                // owned lowercased key; case-sensitive can borrow directly.
                let lowered;
                let lookup_key: &str = if self.config.ignore_case {
                    lowered = value_str.to_lowercase();
                    &lowered
                } else {
                    value_str
                };

                if let Some(rule_indices) = self.ruleset_map.get(lookup_key) {
                    for &rule_idx in rule_indices {
                        matches.push((tagged_span.span, rule_idx));
                    }
                }
            }
        }

        matches
    }

    /// Apply the configured conflict resolution strategy.
    fn resolve_conflicts(&self, sorted: &[MatchEntry]) -> Vec<MatchEntry> {
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
    fn extract_group_priority(&self, entries: &[MatchEntry]) -> (Vec<i32>, Vec<i32>) {
        let groups: Vec<i32> = entries
            .iter()
            .map(|(_, rule_idx)| self.rules[*rule_idx].group as i32)
            .collect();
        let priorities: Vec<i32> = entries
            .iter()
            .map(|(_, rule_idx)| self.rules[*rule_idx].priority)
            .collect();
        (groups, priorities)
    }

    /// Build the final TagResult from resolved matches.
    fn build_result(&self, resolved: &[MatchEntry]) -> TagResult {
        let mut spans: Vec<TaggedSpan> = Vec::new();

        for &(match_span, rule_idx) in resolved {
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
                    if self.config.ambiguous_output_layer {
                        last.annotations.push(annotation);
                    }
                    continue;
                }
            }
            spans.push(TaggedSpan {
                span: match_span,
                annotations: vec![annotation],
            });
        }

        TagResult {
            name: self.config.output_layer.clone(),
            attributes: self.config.output_attributes.clone(),
            ambiguous: self.config.ambiguous_output_layer,
            spans,
        }
    }

    /// Check if rules have inconsistent attribute sets.
    pub fn missing_attributes(&self) -> bool {
        let attrs: Vec<&HashMap<String, AnnotationValue>> =
            self.rules.iter().map(|r| &r.attributes).collect();
        has_missing_attributes(&attrs)
    }

    /// Return a map of pattern strings to their rule indices.
    ///
    /// Returns a reference to the pre-built lookup map, avoiding
    /// redundant recomputation.
    pub fn rule_map(&self) -> &HashMap<String, Vec<usize>> {
        &self.ruleset_map
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MatchSpan;

    fn make_input_layer(spans: Vec<(usize, usize, Vec<HashMap<String, AnnotationValue>>)>) -> TagResult {
        TagResult {
            name: "input".to_string(),
            attributes: vec!["lemma".to_string()],
            ambiguous: true,
            spans: spans
                .into_iter()
                .map(|(start, end, anns)| TaggedSpan {
                    span: MatchSpan::new(start, end),
                    annotations: anns
                        .into_iter()
                        .map(|a| Annotation(a))
                        .collect(),
                })
                .collect(),
        }
    }

    fn default_config() -> SpanTaggerConfig {
        SpanTaggerConfig {
            output_layer: "tagged".to_string(),
            input_attribute: "lemma".to_string(),
            output_attributes: vec![],
            conflict_strategy: ConflictStrategy::KeepAll,
            ignore_case: false,
            group_attribute: None,
            priority_attribute: None,
            pattern_attribute: None,
            ambiguous_output_layer: true,
            unique_patterns: false,
        }
    }

    #[test]
    fn test_basic_matching() {
        let rules = vec![
            make_span_rule(
                "cat",
                HashMap::from([("type".to_string(), AnnotationValue::Str("animal".to_string()))]),
                0,
                0,
            ),
        ];
        let config = SpanTaggerConfig {
            output_attributes: vec!["type".to_string()],
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 3, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("cat".to_string()))])]),
            (4, 7, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("dog".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 3));
        assert_eq!(
            result.spans[0].annotations[0].0.get("type"),
            Some(&AnnotationValue::Str("animal".to_string()))
        );
    }

    #[test]
    fn test_no_match() {
        let rules = vec![
            make_span_rule("cat", HashMap::new(), 0, 0),
        ];
        let tagger = SpanTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 3, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("dog".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_ignore_case() {
        let rules = vec![
            make_span_rule(
                "cat",
                HashMap::from([("type".to_string(), AnnotationValue::Str("animal".to_string()))]),
                0,
                0,
            ),
        ];
        let config = SpanTaggerConfig {
            output_attributes: vec!["type".to_string()],
            ignore_case: true,
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 3, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("Cat".to_string()))])]),
            (4, 7, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("CAT".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 2);
    }

    #[test]
    fn test_multiple_rules_same_pattern() {
        let rules = vec![
            make_span_rule(
                "bank",
                HashMap::from([("type".to_string(), AnnotationValue::Str("finance".to_string()))]),
                0,
                0,
            ),
            make_span_rule(
                "bank",
                HashMap::from([("type".to_string(), AnnotationValue::Str("river".to_string()))]),
                0,
                1,
            ),
        ];
        let config = SpanTaggerConfig {
            output_attributes: vec!["type".to_string()],
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 4, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("bank".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].annotations.len(), 2);
    }

    #[test]
    fn test_non_ambiguous_output() {
        let rules = vec![
            make_span_rule("x", HashMap::new(), 0, 0),
            make_span_rule("x", HashMap::new(), 0, 1),
        ];
        let config = SpanTaggerConfig {
            ambiguous_output_layer: false,
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("x".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].annotations.len(), 1);
        assert!(!result.ambiguous);
    }

    #[test]
    fn test_conflict_keep_maximal() {
        let rules = vec![
            make_span_rule("a", HashMap::new(), 0, 0),
            make_span_rule("b", HashMap::new(), 0, 0),
        ];
        let config = SpanTaggerConfig {
            conflict_strategy: ConflictStrategy::KeepMaximal,
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        // Span (0,5) contains span (1,3)
        let input = make_input_layer(vec![
            (0, 5, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("a".to_string()))])]),
            (1, 3, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("b".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    }

    #[test]
    fn test_conflict_keep_minimal() {
        let rules = vec![
            make_span_rule("a", HashMap::new(), 0, 0),
            make_span_rule("b", HashMap::new(), 0, 0),
        ];
        let config = SpanTaggerConfig {
            conflict_strategy: ConflictStrategy::KeepMinimal,
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        // Span (0,5) contains span (1,3)
        let input = make_input_layer(vec![
            (0, 5, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("a".to_string()))])]),
            (1, 3, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("b".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(1, 3));
    }

    #[test]
    fn test_group_priority_pattern_attributes() {
        let rules = vec![
            make_span_rule(
                "cat",
                HashMap::from([("type".to_string(), AnnotationValue::Str("animal".to_string()))]),
                5,
                2,
            ),
        ];
        let config = SpanTaggerConfig {
            output_attributes: vec!["type".to_string()],
            group_attribute: Some("_group_".to_string()),
            priority_attribute: Some("_priority_".to_string()),
            pattern_attribute: Some("_pattern_".to_string()),
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 3, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("cat".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        let ann = &result.spans[0].annotations[0];
        assert_eq!(ann.0.get("_group_"), Some(&AnnotationValue::Int(5)));
        assert_eq!(ann.0.get("_priority_"), Some(&AnnotationValue::Int(2)));
        assert_eq!(
            ann.0.get("_pattern_"),
            Some(&AnnotationValue::Str("cat".to_string()))
        );
    }

    #[test]
    fn test_unique_patterns_enforced() {
        let rules = vec![
            make_span_rule("x", HashMap::new(), 0, 0),
            make_span_rule("x", HashMap::new(), 0, 1),
        ];
        let config = SpanTaggerConfig {
            unique_patterns: true,
            ..default_config()
        };
        let result = SpanTagger::new(rules, config);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Duplicate pattern"));
    }

    #[test]
    fn test_missing_attributes() {
        let rules = vec![
            make_span_rule(
                "a",
                HashMap::from([("x".to_string(), AnnotationValue::Int(1))]),
                0,
                0,
            ),
            make_span_rule(
                "b",
                HashMap::from([("y".to_string(), AnnotationValue::Int(2))]),
                0,
                0,
            ),
        ];
        let tagger = SpanTagger::new(rules, default_config()).unwrap();
        assert!(tagger.missing_attributes());
    }

    #[test]
    fn test_missing_attributes_consistent() {
        let rules = vec![
            make_span_rule(
                "a",
                HashMap::from([("x".to_string(), AnnotationValue::Int(1))]),
                0,
                0,
            ),
            make_span_rule(
                "b",
                HashMap::from([("x".to_string(), AnnotationValue::Int(2))]),
                0,
                0,
            ),
        ];
        let tagger = SpanTagger::new(rules, default_config()).unwrap();
        assert!(!tagger.missing_attributes());
    }

    #[test]
    fn test_null_attribute_skipped() {
        let rules = vec![
            make_span_rule("x", HashMap::new(), 0, 0),
        ];
        let tagger = SpanTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Null)])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_missing_attribute_skipped() {
        let rules = vec![
            make_span_rule("x", HashMap::new(), 0, 0),
        ];
        let tagger = SpanTagger::new(rules, default_config()).unwrap();

        // Annotation has no "lemma" key at all.
        let input = make_input_layer(vec![
            (0, 1, vec![HashMap::from([("other".to_string(), AnnotationValue::Str("x".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_int_attribute_value_matching() {
        let rules = vec![
            make_span_rule(
                "42",
                HashMap::from([("label".to_string(), AnnotationValue::Str("found".to_string()))]),
                0,
                0,
            ),
        ];
        let config = SpanTaggerConfig {
            input_attribute: "count".to_string(),
            output_attributes: vec!["label".to_string()],
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let input = TagResult {
            name: "input".to_string(),
            attributes: vec!["count".to_string()],
            ambiguous: true,
            spans: vec![TaggedSpan {
                span: MatchSpan::new(0, 5),
                annotations: vec![Annotation(HashMap::from([
                    ("count".to_string(), AnnotationValue::Int(42)),
                ]))],
            }],
        };

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
    }

    #[test]
    fn test_normalize_fills_missing() {
        let rules = vec![
            make_span_rule(
                "a",
                HashMap::from([("x".to_string(), AnnotationValue::Int(1))]),
                0,
                0,
            ),
        ];
        let config = SpanTaggerConfig {
            output_attributes: vec!["x".to_string(), "y".to_string()],
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("a".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        let ann = &result.spans[0].annotations[0];
        assert_eq!(ann.0.get("x"), Some(&AnnotationValue::Int(1)));
        assert_eq!(ann.0.get("y"), Some(&AnnotationValue::Null));
    }

    #[test]
    fn test_rule_map() {
        let rules = vec![
            make_span_rule("a", HashMap::new(), 0, 0),
            make_span_rule("b", HashMap::new(), 0, 0),
            make_span_rule("a", HashMap::new(), 0, 1),
        ];
        let tagger = SpanTagger::new(rules, default_config()).unwrap();
        let map = tagger.rule_map();
        assert_eq!(map.get("a").unwrap().len(), 2);
        assert_eq!(map.get("b").unwrap().len(), 1);
    }

    #[test]
    fn test_priority_conflict_resolution() {
        let rules = vec![
            make_span_rule("a", HashMap::new(), 0, 0),
            make_span_rule("b", HashMap::new(), 0, 1),
        ];
        let config = SpanTaggerConfig {
            conflict_strategy: ConflictStrategy::KeepAllExceptPriority,
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        // Two overlapping spans in the same group, different priorities.
        let input = make_input_layer(vec![
            (0, 5, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("a".to_string()))])]),
            (3, 8, vec![HashMap::from([("lemma".to_string(), AnnotationValue::Str("b".to_string()))])]),
        ]);

        let result = tagger.tag(&input);
        // Priority 1 should be removed (higher number = lower precedence).
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    }

    #[test]
    fn test_ambiguous_input_annotations() {
        // Input span has multiple annotations; each should be checked.
        let rules = vec![
            make_span_rule(
                "cat",
                HashMap::from([("type".to_string(), AnnotationValue::Str("animal".to_string()))]),
                0,
                0,
            ),
        ];
        let config = SpanTaggerConfig {
            output_attributes: vec!["type".to_string()],
            ..default_config()
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 3, vec![
                HashMap::from([("lemma".to_string(), AnnotationValue::Str("dog".to_string()))]),
                HashMap::from([("lemma".to_string(), AnnotationValue::Str("cat".to_string()))]),
            ]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].annotations.len(), 1);
    }

    #[test]
    fn test_pipeline_regex_then_span() {
        // Simulate: RegexTagger produces a layer, SpanTagger processes it.
        let regex_output = TagResult {
            name: "tokens".to_string(),
            attributes: vec!["lemma".to_string()],
            ambiguous: true,
            spans: vec![
                TaggedSpan {
                    span: MatchSpan::new(0, 5),
                    annotations: vec![Annotation(HashMap::from([
                        ("lemma".to_string(), AnnotationValue::Str("tundma".to_string())),
                    ]))],
                },
                TaggedSpan {
                    span: MatchSpan::new(6, 11),
                    annotations: vec![Annotation(HashMap::from([
                        ("lemma".to_string(), AnnotationValue::Str("päike".to_string())),
                    ]))],
                },
                TaggedSpan {
                    span: MatchSpan::new(12, 19),
                    annotations: vec![Annotation(HashMap::from([
                        ("lemma".to_string(), AnnotationValue::Str("inimene".to_string())),
                    ]))],
                },
            ],
        };

        let rules = vec![
            make_span_rule(
                "tundma",
                HashMap::from([("value".to_string(), AnnotationValue::Str("T".to_string()))]),
                0,
                1,
            ),
            make_span_rule(
                "päike",
                HashMap::from([("value".to_string(), AnnotationValue::Str("P".to_string()))]),
                0,
                2,
            ),
        ];
        let config = SpanTaggerConfig {
            output_layer: "tagged_tokens".to_string(),
            input_attribute: "lemma".to_string(),
            output_attributes: vec!["value".to_string()],
            conflict_strategy: ConflictStrategy::KeepAll,
            ignore_case: false,
            group_attribute: None,
            priority_attribute: None,
            pattern_attribute: None,
            ambiguous_output_layer: true,
            unique_patterns: false,
        };
        let tagger = SpanTagger::new(rules, config).unwrap();

        let result = tagger.tag(&regex_output);
        assert_eq!(result.name, "tagged_tokens");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
        assert_eq!(
            result.spans[0].annotations[0].0.get("value"),
            Some(&AnnotationValue::Str("T".to_string()))
        );
        assert_eq!(result.spans[1].span, MatchSpan::new(6, 11));
        assert_eq!(
            result.spans[1].annotations[0].0.get("value"),
            Some(&AnnotationValue::Str("P".to_string()))
        );
    }
}
