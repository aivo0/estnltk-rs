use std::collections::HashMap;

use crate::byte_char::byte_to_char_map;
use crate::conflict::{
    conflict_priority_resolver, keep_maximal_matches, keep_minimal_matches, MatchEntry,
};
use crate::types::{
    has_missing_attributes, normalize_annotation, Annotation, AnnotationValue, ConflictStrategy,
    ExtractionRule, MatchSpan, TagResult, TaggedSpan, TaggerConfig,
};

/// The core regex tagger — Rust equivalent of EstNLTK's `RegexTagger`.
pub struct RegexTagger {
    pub rules: Vec<ExtractionRule>,
    pub config: TaggerConfig,
}

impl RegexTagger {
    /// Create a new tagger, validating configuration.
    pub fn new(rules: Vec<ExtractionRule>, config: TaggerConfig) -> Result<Self, String> {
        for (i, rule) in rules.iter().enumerate() {
            if rule.group != 0 {
                return Err(format!(
                    "Rule {}: group={} not supported. resharp has no capture groups; only group=0 (full match) is allowed.",
                    i, rule.group
                ));
            }
        }
        Ok(Self { rules, config })
    }

    /// Run the full tagging pipeline on a text string.
    pub fn tag(&self, text: &str) -> TagResult {
        let raw_text = if self.config.lowercase_text {
            text.to_lowercase()
        } else {
            text.to_string()
        };

        // Step 1: Extract all matches with byte→char conversion.
        let mut all_matches = self.extract_matches(&raw_text);

        // Step 2: Sort canonically by (start, end).
        all_matches.sort_by_key(|&(span, _)| (span.start, span.end));

        // Step 3: Apply conflict resolution.
        let resolved = self.resolve_conflicts(&all_matches);

        // Step 4: Build TagResult.
        self.build_result(&resolved)
    }

    /// Extract raw matches from all rules, converting byte→char offsets.
    fn extract_matches(&self, text: &str) -> Vec<MatchEntry> {
        let b2c = byte_to_char_map(text);
        let text_bytes = text.as_bytes();
        let mut matches = Vec::new();

        for (rule_idx, rule) in self.rules.iter().enumerate() {
            let found = match rule.compiled.find_all(text_bytes) {
                Ok(v) => v,
                Err(_) => continue,
            };
            for m in found {
                let char_start = b2c[m.start];
                let char_end = b2c[m.end];
                // Skip zero-length matches.
                if char_start == char_end {
                    continue;
                }
                matches.push((MatchSpan::new(char_start, char_end), rule_idx));
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
        // Group consecutive matches at the same span (for ambiguous layers).
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
                    last.annotations.push(annotation);
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

/// Convenience: build an ExtractionRule from components.
pub fn make_rule(
    pattern: &str,
    attributes: HashMap<String, AnnotationValue>,
    group: u32,
    priority: i32,
) -> Result<ExtractionRule, String> {
    let compiled = resharp::Regex::new(pattern).map_err(|e| format!("Regex compile error for '{}': {}", pattern, e))?;
    Ok(ExtractionRule {
        pattern_str: pattern.to_string(),
        compiled,
        attributes,
        group,
        priority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> TaggerConfig {
        TaggerConfig {
            output_layer: "test".to_string(),
            output_attributes: vec![],
            conflict_strategy: ConflictStrategy::KeepAll,
            lowercase_text: false,
            group_attribute: None,
            priority_attribute: None,
            pattern_attribute: None,
        }
    }

    #[test]
    fn test_simple_match() {
        let rule = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("say hello world");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(4, 9));
    }

    #[test]
    fn test_no_match() {
        let rule = make_rule("xyz", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("hello world");
        assert_eq!(result.spans.len(), 0);
    }

    #[test]
    fn test_multiple_matches() {
        let rule = make_rule("ab", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("ab cd ab");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 2));
        assert_eq!(result.spans[1].span, MatchSpan::new(6, 8));
    }

    #[test]
    fn test_estonian_multibyte() {
        // "öö" in "Tüüpiline öökülma näide"
        let rule = make_rule("öö", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![rule], default_config()).unwrap();
        let result = tagger.tag("Tüüpiline öökülma näide");
        assert_eq!(result.spans.len(), 1);
        // char offsets: T(0) ü(1) ü(2) p(3) i(4) l(5) i(6) n(7) e(8) (9)
        //               ö(10) ö(11) k(12) ü(13) l(14) m(15) a(16) (17)
        //               n(18) ä(19) i(20) d(21) e(22)
        assert_eq!(result.spans[0].span, MatchSpan::new(10, 12));
    }

    #[test]
    fn test_lowercase_flag() {
        let rule = make_rule("hello", HashMap::new(), 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.lowercase_text = true;
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("HELLO world");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    }

    #[test]
    fn test_group_nonzero_rejected() {
        let mut rule = make_rule("test", HashMap::new(), 0, 0).unwrap();
        rule.group = 1;
        let result = RegexTagger::new(vec![rule], default_config());
        assert!(result.is_err());
    }

    #[test]
    fn test_attributes_propagated() {
        let mut attrs = HashMap::new();
        attrs.insert("type".to_string(), AnnotationValue::Str("number".to_string()));
        let rule = make_rule("[0-9]+", attrs, 0, 0).unwrap();
        let mut cfg = default_config();
        cfg.output_attributes = vec!["type".to_string()];
        let tagger = RegexTagger::new(vec![rule], cfg).unwrap();
        let result = tagger.tag("abc 123 def");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(
            result.spans[0].annotations[0].0.get("type"),
            Some(&AnnotationValue::Str("number".to_string()))
        );
    }

    #[test]
    fn test_muna_ja_kana_keep_all() {
        // Mirrors test_custom_conflict_resolver.py regex test
        let r1 = make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("ja", HashMap::new(), 0, 0).unwrap();
        let r3 = make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.conflict_strategy = ConflictStrategy::KeepAll;
        cfg.lowercase_text = true;

        let tagger = RegexTagger::new(vec![r1, r2, r3], cfg).unwrap();
        let result = tagger.tag("Muna ja kana.");
        assert_eq!(result.spans.len(), 3);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 7));
        assert_eq!(result.spans[1].span, MatchSpan::new(5, 7));
        assert_eq!(result.spans[2].span, MatchSpan::new(5, 12));
    }

    #[test]
    fn test_muna_ja_kana_keep_maximal() {
        let r1 = make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("ja", HashMap::new(), 0, 0).unwrap();
        let r3 = make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.conflict_strategy = ConflictStrategy::KeepMaximal;
        cfg.lowercase_text = true;

        let tagger = RegexTagger::new(vec![r1, r2, r3], cfg).unwrap();
        let result = tagger.tag("Muna ja kana.");
        assert_eq!(result.spans.len(), 2);
        assert_eq!(result.spans[0].span, MatchSpan::new(0, 7));
        assert_eq!(result.spans[1].span, MatchSpan::new(5, 12));
    }

    #[test]
    fn test_missing_attributes_false_consistent() {
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("x".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("y".to_string()));
        let r1 = make_rule("aaa", a1, 0, 0).unwrap();
        let r2 = make_rule("bbb", a2, 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1, r2], default_config()).unwrap();
        assert!(!tagger.missing_attributes());
    }

    #[test]
    fn test_missing_attributes_true_inconsistent() {
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("x".to_string()));
        a1.insert("color".to_string(), AnnotationValue::Str("red".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("y".to_string()));
        let r1 = make_rule("aaa", a1, 0, 0).unwrap();
        let r2 = make_rule("bbb", a2, 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1, r2], default_config()).unwrap();
        assert!(tagger.missing_attributes());
    }

    #[test]
    fn test_missing_attributes_single_rule() {
        let r1 = make_rule("aaa", HashMap::new(), 0, 0).unwrap();
        let tagger = RegexTagger::new(vec![r1], default_config()).unwrap();
        assert!(!tagger.missing_attributes());
    }

    #[test]
    fn test_missing_attributes_no_rules() {
        let tagger = RegexTagger::new(vec![], default_config()).unwrap();
        assert!(!tagger.missing_attributes());
    }

    #[test]
    fn test_normalize_annotations_fills_null() {
        // Rule 1 has {type, color}, rule 2 has {type} only.
        // output_attributes = ["type", "color"].
        // Rule 2's annotation should get color=Null.
        let mut a1 = HashMap::new();
        a1.insert("type".to_string(), AnnotationValue::Str("email".to_string()));
        a1.insert("color".to_string(), AnnotationValue::Str("red".to_string()));
        let mut a2 = HashMap::new();
        a2.insert("type".to_string(), AnnotationValue::Str("url".to_string()));

        let r1 = make_rule("aaa", a1, 0, 0).unwrap();
        let r2 = make_rule("bbb", a2, 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.output_attributes = vec!["type".to_string(), "color".to_string()];

        let tagger = RegexTagger::new(vec![r1, r2], cfg).unwrap();
        let result = tagger.tag("aaa bbb");

        assert_eq!(result.spans.len(), 2);
        // First span: rule 1 has both attributes
        assert_eq!(
            result.spans[0].annotations[0].0.get("type"),
            Some(&AnnotationValue::Str("email".to_string()))
        );
        assert_eq!(
            result.spans[0].annotations[0].0.get("color"),
            Some(&AnnotationValue::Str("red".to_string()))
        );
        // Second span: rule 2 should have color=Null
        assert_eq!(
            result.spans[1].annotations[0].0.get("type"),
            Some(&AnnotationValue::Str("url".to_string()))
        );
        assert_eq!(
            result.spans[1].annotations[0].0.get("color"),
            Some(&AnnotationValue::Null)
        );
    }

    #[test]
    fn test_muna_ja_kana_keep_minimal() {
        let r1 = make_rule("m..a.ja", HashMap::new(), 0, 0).unwrap();
        let r2 = make_rule("ja", HashMap::new(), 0, 0).unwrap();
        let r3 = make_rule("ja.k..a", HashMap::new(), 0, 0).unwrap();

        let mut cfg = default_config();
        cfg.conflict_strategy = ConflictStrategy::KeepMinimal;
        cfg.lowercase_text = true;

        let tagger = RegexTagger::new(vec![r1, r2, r3], cfg).unwrap();
        let result = tagger.tag("Muna ja kana.");
        assert_eq!(result.spans.len(), 1);
        assert_eq!(result.spans[0].span, MatchSpan::new(5, 7));
    }
}
