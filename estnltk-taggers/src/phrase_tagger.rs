use std::collections::{HashMap, HashSet};

use estnltk_core::{
    conflict_priority_resolver, keep_maximal_matches, keep_minimal_matches, MatchEntry,
};
use estnltk_core::{
    check_unique_phrase_patterns, has_missing_attributes, normalize_annotation, Annotation,
    AnnotationValue, ConflictStrategy, EnvelopingTaggedSpan, MatchSpan, TaggerError,
    PhraseTagResult, TagResult,
};

/// A rule for the PhraseTagger — maps a phrase pattern (tuple of strings) to
/// static attributes.
#[derive(Debug, Clone)]
pub struct PhraseRule {
    /// The phrase pattern as a sequence of strings, e.g., `["euroopa", "liit"]`.
    pub pattern: Vec<String>,
    pub attributes: HashMap<String, AnnotationValue>,
    pub group: u32,
    pub priority: i32,
}

/// Create a PhraseRule from components.
pub fn make_phrase_rule(
    pattern: Vec<String>,
    attributes: HashMap<String, AnnotationValue>,
    group: u32,
    priority: i32,
) -> PhraseRule {
    PhraseRule {
        pattern,
        attributes,
        group,
        priority,
    }
}

/// Configuration for the PhraseTagger.
#[derive(Debug)]
pub struct PhraseTaggerConfig {
    pub output_layer: String,
    pub input_attribute: String,
    pub output_attributes: Vec<String>,
    pub conflict_strategy: ConflictStrategy,
    pub ignore_case: bool,
    /// Name of the attribute storing the matched phrase tuple (default: "phrase").
    pub phrase_attribute: Option<String>,
    pub group_attribute: Option<String>,
    pub priority_attribute: Option<String>,
    pub pattern_attribute: Option<String>,
    pub ambiguous_output_layer: bool,
    pub unique_patterns: bool,
}

/// The PhraseTagger — Rust equivalent of EstNLTK's `PhraseTagger`.
///
/// Matches sequential attribute values from an input layer against a ruleset of
/// phrase patterns (tuples of strings).  For each matching sequence of consecutive
/// spans, creates an enveloping annotation that wraps the constituent spans.
///
/// Input is a `TagResult` (output of RegexTagger, SubstringTagger, SpanTagger,
/// or another tagger), rather than EstNLTK's `Layer` object.
#[derive(Debug)]
pub struct PhraseTagger {
    pub rules: Vec<PhraseRule>,
    /// Maps phrase pattern (as Vec<String>) → list of rule indices.
    static_ruleset_map: HashMap<Vec<String>, Vec<usize>>,
    /// Maps first word of phrase → set of tail sequences.
    /// Used for efficient O(1) lookup of candidate phrases starting at each position.
    heads: HashMap<String, HashSet<Vec<String>>>,
    pub config: PhraseTaggerConfig,
}

impl PhraseTagger {
    /// Create a new PhraseTagger, validating configuration.
    pub fn new(rules: Vec<PhraseRule>, config: PhraseTaggerConfig) -> Result<Self, TaggerError> {
        // Validate: each pattern must have at least one element.
        for (i, rule) in rules.iter().enumerate() {
            if rule.pattern.is_empty() {
                return Err(TaggerError::Config(format!(
                    "Rule {} has an empty pattern; phrases must have at least one word", i
                )));
            }
        }

        // Enforce unique patterns if configured.
        if config.unique_patterns {
            let patterns: Vec<&[String]> = rules.iter().map(|r| r.pattern.as_slice()).collect();
            check_unique_phrase_patterns(&patterns, config.ignore_case)?;
        }

        // Build the pattern → rule-indices lookup map.
        let mut static_ruleset_map: HashMap<Vec<String>, Vec<usize>> = HashMap::new();
        for (i, rule) in rules.iter().enumerate() {
            let key: Vec<String> = if config.ignore_case {
                rule.pattern.iter().map(|s| s.to_lowercase()).collect()
            } else {
                rule.pattern.clone()
            };
            static_ruleset_map.entry(key).or_default().push(i);
        }

        // Build the heads index: first word → set of tail sequences.
        let mut heads: HashMap<String, HashSet<Vec<String>>> = HashMap::new();
        for phrase in static_ruleset_map.keys() {
            let head = &phrase[0];
            let tail: Vec<String> = phrase[1..].to_vec();
            heads.entry(head.clone()).or_default().insert(tail);
        }

        Ok(Self {
            rules,
            static_ruleset_map,
            heads,
            config,
        })
    }

    /// Tag an input layer by matching sequential attribute values against phrase patterns.
    pub fn tag(&self, input: &TagResult) -> PhraseTagResult {
        // Step 1: Build value_list — one set of attribute values per input span position.
        let value_list = self.build_value_list(input);

        // Step 2: Extract phrase matches.
        let mut all_matches = self.extract_matches(input, &value_list);

        // Step 3: Sort by (bounding_start, bounding_end).
        all_matches.sort_by_key(|m| (m.bounding_span.start, m.bounding_span.end));

        // Step 4: Apply conflict resolution on bounding spans.
        let resolved = self.resolve_conflicts(&all_matches);

        // Step 5: Build PhraseTagResult.
        self.build_result(&resolved)
    }

    /// Build value_list: for each span position, collect all attribute values
    /// from all annotations into a HashSet.
    fn build_value_list(&self, input: &TagResult) -> Vec<HashSet<String>> {
        let mut value_list = Vec::with_capacity(input.spans.len());

        for tagged_span in &input.spans {
            let mut values = HashSet::new();
            for annotation in &tagged_span.annotations {
                let value_str = match annotation.get(&self.config.input_attribute) {
                    Some(AnnotationValue::Str(s)) => {
                        if self.config.ignore_case {
                            s.to_lowercase()
                        } else {
                            s.clone()
                        }
                    }
                    Some(AnnotationValue::Int(i)) => i.to_string(),
                    Some(AnnotationValue::Float(f)) => f.to_string(),
                    Some(AnnotationValue::Bool(b)) => b.to_string(),
                    Some(AnnotationValue::Null) | Some(AnnotationValue::List(_)) | None => continue,
                };
                values.insert(value_str);
            }
            value_list.push(values);
        }

        value_list
    }

    /// Extract phrase matches using the heads index.
    ///
    /// Direct port of Python `extract_annotations` algorithm.
    fn extract_matches(
        &self,
        input: &TagResult,
        value_list: &[HashSet<String>],
    ) -> Vec<PhraseMatch> {
        let mut matches = Vec::new();

        for (i, values) in value_list.iter().enumerate() {
            for value in values {
                if let Some(tails) = self.heads.get(value) {
                    for tail in tails {
                        // Boundary check: i + len(tail) < len(value_list)
                        // This ensures all tail positions exist.
                        if i + tail.len() < value_list.len() {
                            // Check each tail element against the corresponding position.
                            let mut matched = true;
                            for (j, tail_elem) in tail.iter().enumerate() {
                                if !value_list[i + 1 + j].contains(tail_elem) {
                                    matched = false;
                                    break;
                                }
                            }

                            if matched {
                                // Collect constituent spans.
                                let constituent_spans: Vec<MatchSpan> = input.spans
                                    [i..=i + tail.len()]
                                    .iter()
                                    .map(|s| s.span)
                                    .collect();

                                // Build phrase tuple.
                                let mut phrase = vec![value.clone()];
                                phrase.extend(tail.iter().cloned());

                                // Bounding span.
                                let bounding_span = MatchSpan::new(
                                    constituent_spans.first().unwrap().start,
                                    constituent_spans.last().unwrap().end,
                                );

                                matches.push(PhraseMatch {
                                    constituent_spans,
                                    bounding_span,
                                    phrase,
                                });
                            }
                        } else if tail.is_empty() {
                            // Single-word phrase: i + 0 < len always holds when
                            // i < len, but the generic check covers it.  This
                            // branch handles the edge case where i == len-1 and
                            // tail is empty — the generic condition `i + 0 < len`
                            // is true, so this branch is actually unreachable.
                            // Kept for clarity.
                            let span = input.spans[i].span;
                            matches.push(PhraseMatch {
                                constituent_spans: vec![span],
                                bounding_span: span,
                                phrase: vec![value.clone()],
                            });
                        }
                    }
                }
            }
        }

        matches
    }

    /// Apply conflict resolution using bounding spans.
    ///
    /// Converts PhraseMatches to MatchEntry (bounding_span, synthetic_index)
    /// for the conflict resolution algorithms, then maps back.
    fn resolve_conflicts(&self, sorted: &[PhraseMatch]) -> Vec<PhraseMatch> {
        if sorted.is_empty() {
            return Vec::new();
        }

        // Create MatchEntry pairs using indices as synthetic rule indices.
        let entries: Vec<MatchEntry> = sorted
            .iter()
            .enumerate()
            .map(|(i, m)| (m.bounding_span, i))
            .collect();

        let resolved_indices: Vec<usize> = match self.config.conflict_strategy {
            ConflictStrategy::KeepAll => (0..sorted.len()).collect(),
            ConflictStrategy::KeepMaximal => {
                keep_maximal_matches(&entries)
                    .iter()
                    .map(|&(_, idx)| idx)
                    .collect()
            }
            ConflictStrategy::KeepMinimal => {
                keep_minimal_matches(&entries)
                    .iter()
                    .map(|&(_, idx)| idx)
                    .collect()
            }
            ConflictStrategy::KeepAllExceptPriority => {
                let (groups, priorities) = self.extract_phrase_group_priority(sorted);
                conflict_priority_resolver(&entries, &groups, &priorities)
                    .iter()
                    .map(|&(_, idx)| idx)
                    .collect()
            }
            ConflictStrategy::KeepMaximalExceptPriority => {
                let (groups, priorities) = self.extract_phrase_group_priority(sorted);
                let after_priority =
                    conflict_priority_resolver(&entries, &groups, &priorities);
                keep_maximal_matches(&after_priority)
                    .iter()
                    .map(|&(_, idx)| idx)
                    .collect()
            }
            ConflictStrategy::KeepMinimalExceptPriority => {
                let (groups, priorities) = self.extract_phrase_group_priority(sorted);
                let after_priority =
                    conflict_priority_resolver(&entries, &groups, &priorities);
                keep_minimal_matches(&after_priority)
                    .iter()
                    .map(|&(_, idx)| idx)
                    .collect()
            }
        };

        resolved_indices
            .into_iter()
            .map(|i| sorted[i].clone())
            .collect()
    }

    /// Extract group and priority for each phrase match.
    ///
    /// Uses the first rule's group/priority for each phrase pattern,
    /// matching Python PhraseTagger behavior (`priority_info[1][0][0]`
    /// and `priority_info[1][0][1]`).
    fn extract_phrase_group_priority(&self, matches: &[PhraseMatch]) -> (Vec<i32>, Vec<i32>) {
        let mut groups = Vec::with_capacity(matches.len());
        let mut priorities = Vec::with_capacity(matches.len());

        for m in matches {
            if let Some(rule_indices) = self.static_ruleset_map.get(&m.phrase) {
                let first_rule = &self.rules[rule_indices[0]];
                groups.push(first_rule.group as i32);
                priorities.push(first_rule.priority);
            } else {
                // Shouldn't happen — every match comes from a known phrase.
                groups.push(0);
                priorities.push(0);
            }
        }

        (groups, priorities)
    }

    /// Build the final PhraseTagResult from resolved matches.
    fn build_result(&self, resolved: &[PhraseMatch]) -> PhraseTagResult {
        let mut spans: Vec<EnvelopingTaggedSpan> = Vec::new();

        for phrase_match in resolved {
            // Look up all matching rules for this phrase.
            let rule_indices = match self.static_ruleset_map.get(&phrase_match.phrase) {
                Some(indices) => indices,
                None => continue,
            };

            let mut annotations = Vec::new();
            for &rule_idx in rule_indices {
                let rule = &self.rules[rule_idx];
                let mut annotation = Annotation::new();

                // Copy static attributes from rule.
                for (k, v) in &rule.attributes {
                    annotation.insert(k.clone(), v.clone());
                }

                // Add phrase attribute.
                if let Some(ref attr_name) = self.config.phrase_attribute {
                    let phrase_val = AnnotationValue::List(
                        phrase_match
                            .phrase
                            .iter()
                            .map(|s| AnnotationValue::Str(s.clone()))
                            .collect(),
                    );
                    annotation.insert(attr_name.clone(), phrase_val);
                }

                // Optionally add group/priority/pattern attributes.
                if let Some(ref attr_name) = self.config.group_attribute {
                    annotation.insert(attr_name.clone(), AnnotationValue::Int(rule.group as i64));
                }
                if let Some(ref attr_name) = self.config.priority_attribute {
                    annotation.insert(attr_name.clone(), AnnotationValue::Int(rule.priority as i64));
                }
                if let Some(ref attr_name) = self.config.pattern_attribute {
                    // Pattern attribute stores the phrase tuple (same as phrase_attribute
                    // in Python PhraseTagger).
                    let phrase_val = AnnotationValue::List(
                        phrase_match
                            .phrase
                            .iter()
                            .map(|s| AnnotationValue::Str(s.clone()))
                            .collect(),
                    );
                    annotation.insert(attr_name.clone(), phrase_val);
                }

                // Normalize: fill missing output_attributes with Null.
                normalize_annotation(&mut annotation, &self.config.output_attributes);

                annotations.push(annotation);

                // If non-ambiguous output, only keep the first annotation.
                if !self.config.ambiguous_output_layer {
                    break;
                }
            }

            // Merge into existing enveloping span or create new one.
            if let Some(last) = spans.last_mut() {
                if last.bounding_span == phrase_match.bounding_span {
                    if self.config.ambiguous_output_layer {
                        last.annotations.extend(annotations);
                    }
                    continue;
                }
            }

            spans.push(EnvelopingTaggedSpan {
                spans: phrase_match.constituent_spans.clone(),
                bounding_span: phrase_match.bounding_span,
                annotations,
            });
        }

        PhraseTagResult {
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

    /// Return a reference to the pre-built phrase→rule-indices lookup map.
    pub fn rule_map(&self) -> &HashMap<Vec<String>, Vec<usize>> {
        &self.static_ruleset_map
    }
}

/// Internal: a phrase match before conflict resolution and annotation assembly.
#[derive(Debug, Clone)]
struct PhraseMatch {
    /// The constituent elementary spans.
    constituent_spans: Vec<MatchSpan>,
    /// Bounding span for conflict resolution.
    bounding_span: MatchSpan,
    /// The matched phrase tuple (possibly lowercased).
    phrase: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use estnltk_core::{MatchSpan, TaggedSpan};

    fn make_input_layer(
        spans: Vec<(usize, usize, Vec<HashMap<String, AnnotationValue>>)>,
    ) -> TagResult {
        TagResult {
            name: "morph_analysis".to_string(),
            attributes: vec!["lemma".to_string()],
            ambiguous: true,
            spans: spans
                .into_iter()
                .map(|(start, end, anns)| TaggedSpan {
                    span: MatchSpan::new(start, end),
                    annotations: anns.into_iter().map(Annotation::from).collect(),
                })
                .collect(),
        }
    }

    fn ann(lemma: &str) -> HashMap<String, AnnotationValue> {
        HashMap::from([("lemma".to_string(), AnnotationValue::Str(lemma.to_string()))])
    }

    fn default_config() -> PhraseTaggerConfig {
        PhraseTaggerConfig {
            output_layer: "phrases".to_string(),
            input_attribute: "lemma".to_string(),
            output_attributes: vec!["value".to_string()],
            conflict_strategy: ConflictStrategy::KeepAll,
            ignore_case: false,
            phrase_attribute: Some("phrase".to_string()),
            group_attribute: None,
            priority_attribute: None,
            pattern_attribute: None,
            ambiguous_output_layer: true,
            unique_patterns: false,
        }
    }

    #[test]
    fn test_basic_two_word_phrase() {
        let rules = vec![make_phrase_rule(
            vec!["euroopa".to_string(), "liit".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
            0,
            0,
        )];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 6, vec![ann("varsti")]),
            (7, 12, vec![ann("tulema")]),
            (13, 20, vec![ann("euroopa")]),
            (21, 28, vec![ann("liit")]),
            (29, 38, vec![ann("lahkumine")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.name, "phrases");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].spans.len(), 2);
        assert_eq!(result.spans[0].spans[0], MatchSpan::new(13, 20));
        assert_eq!(result.spans[0].spans[1], MatchSpan::new(21, 28));
        assert_eq!(result.spans[0].bounding_span, MatchSpan::new(13, 28));
        assert_eq!(
            result.spans[0].annotations[0].get("value"),
            Some(&AnnotationValue::Str("ORG".to_string()))
        );
        // Check phrase attribute.
        assert_eq!(
            result.spans[0].annotations[0].get("phrase"),
            Some(&AnnotationValue::List(vec![
                AnnotationValue::Str("euroopa".to_string()),
                AnnotationValue::Str("liit".to_string()),
            ]))
        );
    }

    #[test]
    fn test_single_word_phrase() {
        let rules = vec![make_phrase_rule(
            vec!["eesti".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("LOC".to_string()))]),
            0,
            0,
        )];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 5, vec![ann("eesti")]),
            (6, 10, vec![ann("keel")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].spans.len(), 1);
        assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 5));
    }

    #[test]
    fn test_three_word_phrase() {
        let rules = vec![make_phrase_rule(
            vec![
                "new".to_string(),
                "york".to_string(),
                "city".to_string(),
            ],
            HashMap::from([("value".to_string(), AnnotationValue::Str("LOC".to_string()))]),
            0,
            0,
        )];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 3, vec![ann("new")]),
            (4, 8, vec![ann("york")]),
            (9, 13, vec![ann("city")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].spans.len(), 3);
        assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 13));
    }

    #[test]
    fn test_no_match() {
        let rules = vec![make_phrase_rule(
            vec!["euroopa".to_string(), "liit".to_string()],
            HashMap::new(),
            0,
            0,
        )];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 7, vec![ann("euroopa")]),
            (8, 13, vec![ann("pank")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_empty_input() {
        let rules = vec![make_phrase_rule(
            vec!["a".to_string()],
            HashMap::new(),
            0,
            0,
        )];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = TagResult {
            name: "input".to_string(),
            attributes: vec!["lemma".to_string()],
            ambiguous: true,
            spans: vec![],
        };

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_ambiguous_input_annotations() {
        // Input span has multiple annotations (ambiguous morphology).
        // "Liidust" might have lemmas "liidu" and "liit".
        let rules = vec![
            make_phrase_rule(
                vec!["euroopa".to_string(), "liit".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["euroopa".to_string(), "liidu".to_string()],
                HashMap::from([(
                    "value".to_string(),
                    AnnotationValue::Str("ORG2".to_string()),
                )]),
                0,
                0,
            ),
        ];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (13, 20, vec![ann("euroopa")]),
            (
                21, 28,
                vec![ann("liit"), ann("liidu")],
            ),
        ]);

        let result = tagger.tag(&input);
        // Both phrases match because the second span has both lemmas.
        assert_eq!(result.spans.len(), 1);
        // Both annotations on the same bounding span.
        assert_eq!(result.spans[0].annotations.len(), 2);
    }

    #[test]
    fn test_ignore_case() {
        let rules = vec![make_phrase_rule(
            vec!["euroopa".to_string(), "liit".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
            0,
            0,
        )];
        let config = PhraseTaggerConfig {
            ignore_case: true,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (13, 20, vec![ann("Euroopa")]),
            (21, 28, vec![ann("Liit")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
    }

    #[test]
    fn test_ignore_case_estonian() {
        let rules = vec![make_phrase_rule(
            vec!["põhja".to_string(), "täht".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("STAR".to_string()))]),
            0,
            0,
        )];
        let config = PhraseTaggerConfig {
            ignore_case: true,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 5, vec![ann("Põhja")]),
            (6, 10, vec![ann("Täht")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
    }

    #[test]
    fn test_conflict_keep_maximal() {
        // Two overlapping phrases where bounding spans overlap.
        let rules = vec![
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Int(1))]),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string(), "c".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Int(2))]),
                0,
                0,
            ),
        ];
        let config = PhraseTaggerConfig {
            conflict_strategy: ConflictStrategy::KeepMaximal,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("b")]),
            (4, 5, vec![ann("c")]),
        ]);

        let result = tagger.tag(&input);
        // Only the three-word phrase should survive (it has the maximal bounding span).
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 5));
    }

    #[test]
    fn test_conflict_keep_minimal() {
        let rules = vec![
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Int(1))]),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string(), "c".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Int(2))]),
                0,
                0,
            ),
        ];
        let config = PhraseTaggerConfig {
            conflict_strategy: ConflictStrategy::KeepMinimal,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("b")]),
            (4, 5, vec![ann("c")]),
        ]);

        let result = tagger.tag(&input);
        // Only the two-word phrase should survive (it has the minimal bounding span).
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 3));
    }

    #[test]
    fn test_conflict_keep_all() {
        let rules = vec![
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Int(1))]),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string(), "c".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Int(2))]),
                0,
                0,
            ),
        ];
        let config = PhraseTaggerConfig {
            conflict_strategy: ConflictStrategy::KeepAll,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("b")]),
            (4, 5, vec![ann("c")]),
        ]);

        let result = tagger.tag(&input);
        // Both phrases should survive.
        assert_eq!(result.spans.len(), 2);
    }

    #[test]
    fn test_priority_conflict_resolution() {
        let rules = vec![
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string()],
                HashMap::new(),
                0,
                0, // lower priority number = higher precedence
            ),
            make_phrase_rule(
                vec!["b".to_string(), "c".to_string()],
                HashMap::new(),
                0,
                1, // higher priority number = lower precedence
            ),
        ];
        let config = PhraseTaggerConfig {
            conflict_strategy: ConflictStrategy::KeepAllExceptPriority,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("b")]),
            (4, 5, vec![ann("c")]),
        ]);

        let result = tagger.tag(&input);
        // "a b" (priority 0) overlaps with "b c" (priority 1) in same group.
        // Priority 1 should be removed.
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 3));
    }

    #[test]
    fn test_group_priority_pattern_attributes() {
        let rules = vec![make_phrase_rule(
            vec!["euroopa".to_string(), "liit".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
            5,
            2,
        )];
        let config = PhraseTaggerConfig {
            group_attribute: Some("_group_".to_string()),
            priority_attribute: Some("_priority_".to_string()),
            pattern_attribute: Some("_pattern_".to_string()),
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 7, vec![ann("euroopa")]),
            (8, 12, vec![ann("liit")]),
        ]);

        let result = tagger.tag(&input);
        let a = &result.spans[0].annotations[0];
        assert_eq!(a.get("_group_"), Some(&AnnotationValue::Int(5)));
        assert_eq!(a.get("_priority_"), Some(&AnnotationValue::Int(2)));
        assert_eq!(
            a.get("_pattern_"),
            Some(&AnnotationValue::List(vec![
                AnnotationValue::Str("euroopa".to_string()),
                AnnotationValue::Str("liit".to_string()),
            ]))
        );
    }

    #[test]
    fn test_phrase_attribute_stored() {
        let rules = vec![make_phrase_rule(
            vec!["euroopa".to_string(), "liit".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
            0,
            0,
        )];
        let config = PhraseTaggerConfig {
            phrase_attribute: Some("matched_phrase".to_string()),
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 7, vec![ann("euroopa")]),
            (8, 12, vec![ann("liit")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(
            result.spans[0].annotations[0].get("matched_phrase"),
            Some(&AnnotationValue::List(vec![
                AnnotationValue::Str("euroopa".to_string()),
                AnnotationValue::Str("liit".to_string()),
            ]))
        );
    }

    #[test]
    fn test_no_phrase_attribute() {
        let rules = vec![make_phrase_rule(
            vec!["a".to_string(), "b".to_string()],
            HashMap::from([("value".to_string(), AnnotationValue::Int(1))]),
            0,
            0,
        )];
        let config = PhraseTaggerConfig {
            phrase_attribute: None,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("b")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        // No phrase attribute in annotation.
        assert!(result.spans[0].annotations[0].get("phrase").is_none());
    }

    #[test]
    fn test_rule_ambiguity_same_bounding_span() {
        // Two rules with the same phrase pattern → multiple annotations on same span.
        let rules = vec![
            make_phrase_rule(
                vec!["euroopa".to_string(), "liit".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["euroopa".to_string(), "liit".to_string()],
                HashMap::from([(
                    "value".to_string(),
                    AnnotationValue::Str("ENTITY".to_string()),
                )]),
                0,
                1,
            ),
        ];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 7, vec![ann("euroopa")]),
            (8, 12, vec![ann("liit")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].annotations.len(), 2);
        assert_eq!(
            result.spans[0].annotations[0].get("value"),
            Some(&AnnotationValue::Str("ORG".to_string()))
        );
        assert_eq!(
            result.spans[0].annotations[1].get("value"),
            Some(&AnnotationValue::Str("ENTITY".to_string()))
        );
    }

    #[test]
    fn test_non_ambiguous_output_layer() {
        let rules = vec![
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Int(1))]),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Int(2))]),
                0,
                1,
            ),
        ];
        let config = PhraseTaggerConfig {
            ambiguous_output_layer: false,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("b")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].annotations.len(), 1);
        assert!(!result.ambiguous);
    }

    #[test]
    fn test_unique_patterns_enforced() {
        let rules = vec![
            make_phrase_rule(vec!["a".to_string(), "b".to_string()], HashMap::new(), 0, 0),
            make_phrase_rule(vec!["a".to_string(), "b".to_string()], HashMap::new(), 0, 1),
        ];
        let config = PhraseTaggerConfig {
            unique_patterns: true,
            ..default_config()
        };
        let result = PhraseTagger::new(rules, config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate phrase pattern"));
    }

    #[test]
    fn test_missing_attributes() {
        let rules = vec![
            make_phrase_rule(
                vec!["a".to_string()],
                HashMap::from([("x".to_string(), AnnotationValue::Int(1))]),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["b".to_string()],
                HashMap::from([("y".to_string(), AnnotationValue::Int(2))]),
                0,
                0,
            ),
        ];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();
        assert!(tagger.missing_attributes());
    }

    #[test]
    fn test_normalize_fills_missing_attributes() {
        let rules = vec![make_phrase_rule(
            vec!["a".to_string(), "b".to_string()],
            HashMap::from([("x".to_string(), AnnotationValue::Int(1))]),
            0,
            0,
        )];
        let config = PhraseTaggerConfig {
            output_attributes: vec!["x".to_string(), "y".to_string()],
            phrase_attribute: None,
            ..default_config()
        };
        let tagger = PhraseTagger::new(rules, config).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("b")]),
        ]);

        let result = tagger.tag(&input);
        let a = &result.spans[0].annotations[0];
        assert_eq!(a.get("x"), Some(&AnnotationValue::Int(1)));
        assert_eq!(a.get("y"), Some(&AnnotationValue::Null));
    }

    #[test]
    fn test_empty_pattern_rejected() {
        let rules = vec![make_phrase_rule(vec![], HashMap::new(), 0, 0)];
        let result = PhraseTagger::new(rules, default_config());
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_phrases_in_text() {
        let rules = vec![
            make_phrase_rule(
                vec!["euroopa".to_string(), "liit".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Str("ORG".to_string()))]),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["eesti".to_string(), "vabariik".to_string()],
                HashMap::from([("value".to_string(), AnnotationValue::Str("LOC".to_string()))]),
                0,
                0,
            ),
        ];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 5, vec![ann("eesti")]),
            (6, 14, vec![ann("vabariik")]),
            (15, 17, vec![ann("ja")]),
            (18, 25, vec![ann("euroopa")]),
            (26, 30, vec![ann("liit")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].bounding_span, MatchSpan::new(0, 14));
        assert_eq!(result.spans[1].bounding_span, MatchSpan::new(18, 30));
    }

    #[test]
    fn test_phrase_at_end_of_input() {
        // Phrase pattern extends to the very last span.
        let rules = vec![make_phrase_rule(
            vec!["x".to_string(), "y".to_string()],
            HashMap::new(),
            0,
            0,
        )];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("x")]),
            (4, 5, vec![ann("y")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].bounding_span, MatchSpan::new(2, 5));
    }

    #[test]
    fn test_phrase_would_exceed_input_boundary() {
        // Phrase head matches at last position but tail would exceed input.
        let rules = vec![make_phrase_rule(
            vec!["x".to_string(), "y".to_string()],
            HashMap::new(),
            0,
            0,
        )];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![
            (0, 1, vec![ann("a")]),
            (2, 3, vec![ann("x")]),
        ]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_null_attribute_skipped() {
        let rules = vec![make_phrase_rule(
            vec!["x".to_string()],
            HashMap::new(),
            0,
            0,
        )];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();

        let input = make_input_layer(vec![(
            0,
            1,
            vec![HashMap::from([(
                "lemma".to_string(),
                AnnotationValue::Null,
            )])],
        )]);

        let result = tagger.tag(&input);
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_rule_map() {
        let rules = vec![
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string()],
                HashMap::new(),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["c".to_string()],
                HashMap::new(),
                0,
                0,
            ),
            make_phrase_rule(
                vec!["a".to_string(), "b".to_string()],
                HashMap::new(),
                0,
                1,
            ),
        ];
        let tagger = PhraseTagger::new(rules, default_config()).unwrap();
        let map = tagger.rule_map();
        assert_eq!(
            map.get(&vec!["a".to_string(), "b".to_string()])
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            map.get(&vec!["c".to_string()]).unwrap().len(),
            1
        );
    }
}
