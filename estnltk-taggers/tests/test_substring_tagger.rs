use std::collections::HashMap;

use estnltk_taggers::{SubstringRule, SubstringTagger};
use estnltk_core::*;

fn default_config() -> TaggerConfig {
    TaggerConfig {
        common: CommonConfig {
            output_layer: "test".to_string(),
            conflict_strategy: ConflictStrategy::KeepMaximal,
            ..CommonConfig::default()
        },
        lowercase_text: false,
        overlapped: false,
        match_attribute: None,
    }
}

#[test]
fn test_matching_without_separators() {
    let rules = vec![
        SubstringRule::new("first", HashMap::new(), 0, 0),
        SubstringRule::new("firs", HashMap::new(), 0, 0),
        SubstringRule::new("irst", HashMap::new(), 0, 0),
        SubstringRule::new("last", HashMap::new(), 0, 0),
    ];
    let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
    let result = tagger.tag("first second last");
    assert_eq!(result.spans.len(), 2);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    assert_eq!(result.spans[1].span, MatchSpan::new(13, 17));
}

#[test]
fn test_matching_ignore_case() {
    let rules = vec![
        SubstringRule::new("First", HashMap::new(), 0, 0),
        SubstringRule::new("firs", HashMap::new(), 0, 0),
        SubstringRule::new("irst", HashMap::new(), 0, 0),
        SubstringRule::new("LAST", HashMap::new(), 0, 0),
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
    let rules = vec![SubstringRule::new("match", HashMap::new(), 0, 0)];
    let tagger = SubstringTagger::new(rules, "|", default_config()).unwrap();
    let result = tagger.tag("match|match| match| match| match |match");
    assert_eq!(result.spans.len(), 3);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    assert_eq!(result.spans[1].span, MatchSpan::new(6, 11));
    assert_eq!(result.spans[2].span, MatchSpan::new(34, 39));
}

#[test]
fn test_separator_multiple_chars() {
    let rules = vec![SubstringRule::new("match", HashMap::new(), 0, 0)];
    let tagger = SubstringTagger::new(rules, " ,:", default_config()).unwrap();
    let result = tagger.tag("match match, :match, match");
    assert_eq!(result.spans.len(), 4);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    assert_eq!(result.spans[1].span, MatchSpan::new(6, 11));
    assert_eq!(result.spans[2].span, MatchSpan::new(14, 19));
    assert_eq!(result.spans[3].span, MatchSpan::new(21, 26));
}

#[test]
fn test_annotations_propagated() {
    let mut a1 = HashMap::new();
    a1.insert("a".to_string(), AnnotationValue::Int(1));
    a1.insert("b".to_string(), AnnotationValue::Int(1));
    let mut a2 = HashMap::new();
    a2.insert("b".to_string(), AnnotationValue::Int(2));
    a2.insert("a".to_string(), AnnotationValue::Int(3));
    let mut a3 = HashMap::new();
    a3.insert("a".to_string(), AnnotationValue::Int(3));
    a3.insert("b".to_string(), AnnotationValue::Int(5));

    let rules = vec![
        SubstringRule::new("first", a1, 0, 0),
        SubstringRule::new("second", a2, 0, 0),
        SubstringRule::new("last", a3, 0, 0),
    ];
    let mut cfg = default_config();
    cfg.common.output_attributes = vec!["a".to_string(), "b".to_string()];
    let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
    let result = tagger.tag("first second last");
    assert_eq!(result.spans.len(), 3);
    assert_eq!(result.spans[0].annotations[0].get("a"), Some(&AnnotationValue::Int(1)));
    assert_eq!(result.spans[1].annotations[0].get("b"), Some(&AnnotationValue::Int(2)));
    assert_eq!(result.spans[2].annotations[0].get("a"), Some(&AnnotationValue::Int(3)));
}

#[test]
fn test_keep_minimal_overlapping() {
    let rules = vec![
        SubstringRule::new("abcd", HashMap::new(), 0, 0),
        SubstringRule::new("abc", HashMap::new(), 0, 0),
        SubstringRule::new("bc", HashMap::new(), 0, 0),
        SubstringRule::new("bcd", HashMap::new(), 0, 0),
        SubstringRule::new("bcde", HashMap::new(), 0, 0),
        SubstringRule::new("f", HashMap::new(), 0, 0),
        SubstringRule::new("ef", HashMap::new(), 0, 0),
    ];
    let mut cfg = default_config();
    cfg.common.conflict_strategy = ConflictStrategy::KeepMinimal;
    let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
    let result = tagger.tag("abcdea--efg");
    assert_eq!(result.spans.len(), 2);
    assert_eq!(result.spans[0].span, MatchSpan::new(1, 3));
    assert_eq!(result.spans[1].span, MatchSpan::new(9, 10));
}

#[test]
fn test_keep_maximal_overlapping() {
    let rules = vec![
        SubstringRule::new("abcd", HashMap::new(), 0, 0),
        SubstringRule::new("abc", HashMap::new(), 0, 0),
        SubstringRule::new("bc", HashMap::new(), 0, 0),
        SubstringRule::new("bcd", HashMap::new(), 0, 0),
        SubstringRule::new("bcde", HashMap::new(), 0, 0),
        SubstringRule::new("f", HashMap::new(), 0, 0),
        SubstringRule::new("ef", HashMap::new(), 0, 0),
    ];
    let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
    let result = tagger.tag("abcdea--efg");
    assert_eq!(result.spans.len(), 3);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 4));
    assert_eq!(result.spans[1].span, MatchSpan::new(1, 5));
    assert_eq!(result.spans[2].span, MatchSpan::new(8, 10));
}

#[test]
fn test_keep_all_overlapping() {
    let rules = vec![
        SubstringRule::new("abcd", HashMap::new(), 0, 0),
        SubstringRule::new("abc", HashMap::new(), 0, 0),
        SubstringRule::new("bc", HashMap::new(), 0, 0),
        SubstringRule::new("bcd", HashMap::new(), 0, 0),
        SubstringRule::new("bcde", HashMap::new(), 0, 0),
        SubstringRule::new("f", HashMap::new(), 0, 0),
        SubstringRule::new("ef", HashMap::new(), 0, 0),
    ];
    let mut cfg = default_config();
    cfg.common.conflict_strategy = ConflictStrategy::KeepAll;
    let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
    let result = tagger.tag("abcdea--efg");
    assert_eq!(result.spans.len(), 7);
}

#[test]
fn test_estonian_multibyte() {
    let rules = vec![SubstringRule::new("öö", HashMap::new(), 0, 0)];
    let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
    let result = tagger.tag("Tüüpiline öökülma näide");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(10, 12));
}

#[test]
fn test_ambiguous_two_rules_same_pattern() {
    let mut a1 = HashMap::new();
    a1.insert("type".to_string(), AnnotationValue::Str("capital".to_string()));
    a1.insert("country".to_string(), AnnotationValue::Str("US".to_string()));
    let mut a2 = HashMap::new();
    a2.insert("type".to_string(), AnnotationValue::Str("name".to_string()));
    a2.insert("country".to_string(), AnnotationValue::Str("US".to_string()));

    let rules = vec![
        SubstringRule::new("Washington", a1, 0, 0),
        SubstringRule::new("Washington", a2, 0, 0),
    ];
    let mut cfg = default_config();
    cfg.common.output_attributes = vec!["type".to_string(), "country".to_string()];
    let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
    let result = tagger.tag("Washington");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].annotations.len(), 2);
}

#[test]
fn test_pattern_attribute_injection() {
    let rules = vec![
        SubstringRule::new("hello", HashMap::new(), 0, 0),
        SubstringRule::new("world", HashMap::new(), 0, 0),
    ];
    let mut cfg = default_config();
    cfg.common.pattern_attribute = Some("_pattern".to_string());
    let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
    let result = tagger.tag("hello world");
    assert_eq!(result.spans.len(), 2);
    assert_eq!(
        result.spans[0].annotations[0].get("_pattern"),
        Some(&AnnotationValue::Str("hello".to_string()))
    );
    assert_eq!(
        result.spans[1].annotations[0].get("_pattern"),
        Some(&AnnotationValue::Str("world".to_string()))
    );
}

#[test]
fn test_priority_resolution() {
    let mut a1 = HashMap::new();
    a1.insert("type".to_string(), AnnotationValue::Str("greeting".to_string()));
    let mut a2 = HashMap::new();
    a2.insert("type".to_string(), AnnotationValue::Str("salutation".to_string()));

    let rules = vec![
        SubstringRule::new("hello", a1, 0, 0),
        SubstringRule::new("hell", a2, 0, 1),
    ];
    let mut cfg = default_config();
    cfg.common.conflict_strategy = ConflictStrategy::KeepAllExceptPriority;
    cfg.common.output_attributes = vec!["type".to_string()];
    let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
    let result = tagger.tag("hello world");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    assert_eq!(
        result.spans[0].annotations[0].get("type"),
        Some(&AnnotationValue::Str("greeting".to_string()))
    );
}
