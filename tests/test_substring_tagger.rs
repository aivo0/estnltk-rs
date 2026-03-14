use std::collections::HashMap;

use estnltk_regex_rs::substring_tagger::{make_substring_rule, SubstringTagger};
use estnltk_regex_rs::types::*;

fn default_config() -> TaggerConfig {
    TaggerConfig {
        output_layer: "test".to_string(),
        output_attributes: vec![],
        conflict_strategy: ConflictStrategy::KeepMaximal,
        lowercase_text: false,
        group_attribute: None,
        priority_attribute: None,
        pattern_attribute: None,
        ambiguous_output_layer: true,
    }
}

/// Mirrors Python test_matching_without_separators.
#[test]
fn test_matching_without_separators() {
    let rules = vec![
        make_substring_rule("first", HashMap::new(), 0, 0),
        make_substring_rule("firs", HashMap::new(), 0, 0),
        make_substring_rule("irst", HashMap::new(), 0, 0),
        make_substring_rule("last", HashMap::new(), 0, 0),
    ];
    let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
    let result = tagger.tag("first second last");
    assert_eq!(result.spans.len(), 2, "Maximal matches must be returned");
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    assert_eq!(result.spans[1].span, MatchSpan::new(13, 17));
}

/// Mirrors Python test_matching_without_separators ignore_case part.
#[test]
fn test_matching_ignore_case() {
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

/// Mirrors Python test_separator_effect (pipe separator).
#[test]
fn test_separator_pipe() {
    let rules = vec![make_substring_rule("match", HashMap::new(), 0, 0)];
    let tagger = SubstringTagger::new(rules, "|", default_config()).unwrap();
    let result = tagger.tag("match|match| match| match| match |match");
    assert_eq!(result.spans.len(), 3, "Separators not correctly handled");
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    assert_eq!(result.spans[1].span, MatchSpan::new(6, 11));
    assert_eq!(result.spans[2].span, MatchSpan::new(34, 39));
}

/// Mirrors Python test_separator_effect (multiple separators).
#[test]
fn test_separator_multiple_chars() {
    let rules = vec![make_substring_rule("match", HashMap::new(), 0, 0)];
    let tagger = SubstringTagger::new(rules, " ,:", default_config()).unwrap();
    let result = tagger.tag("match match, :match, match");
    assert_eq!(result.spans.len(), 4, "Multiple separators do not work");
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 5));
    assert_eq!(result.spans[1].span, MatchSpan::new(6, 11));
    assert_eq!(result.spans[2].span, MatchSpan::new(14, 19));
    assert_eq!(result.spans[3].span, MatchSpan::new(21, 26));
}

/// Mirrors Python test_annotations.
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
        result.spans[0].annotations[0].0.get("b"),
        Some(&AnnotationValue::Int(1))
    );
    assert_eq!(
        result.spans[1].annotations[0].0.get("b"),
        Some(&AnnotationValue::Int(2))
    );
    assert_eq!(
        result.spans[1].annotations[0].0.get("a"),
        Some(&AnnotationValue::Int(3))
    );
    assert_eq!(
        result.spans[2].annotations[0].0.get("a"),
        Some(&AnnotationValue::Int(3))
    );
    assert_eq!(
        result.spans[2].annotations[0].0.get("b"),
        Some(&AnnotationValue::Int(5))
    );
}

/// Mirrors Python test_minimal_and_maximal_matching KEEP_MINIMAL.
#[test]
fn test_keep_minimal_overlapping() {
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
    assert_eq!(result.spans.len(), 2, "Minimal matching does not work");
    assert_eq!(result.spans[0].span, MatchSpan::new(1, 3)); // bc
    assert_eq!(result.spans[1].span, MatchSpan::new(9, 10)); // f
}

/// Mirrors Python test_minimal_and_maximal_matching KEEP_MAXIMAL.
#[test]
fn test_keep_maximal_overlapping() {
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
    assert_eq!(result.spans.len(), 3, "Maximal matching does not work");
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 4)); // abcd
    assert_eq!(result.spans[1].span, MatchSpan::new(1, 5)); // bcde
    assert_eq!(result.spans[2].span, MatchSpan::new(8, 10)); // ef
}

/// Mirrors Python test_minimal_and_maximal_matching KEEP_ALL.
#[test]
fn test_keep_all_overlapping() {
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
    assert_eq!(result.spans.len(), 7, "All matches does not work");
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 3)); // abc
    assert_eq!(result.spans[1].span, MatchSpan::new(0, 4)); // abcd
    assert_eq!(result.spans[2].span, MatchSpan::new(1, 3)); // bc
    assert_eq!(result.spans[3].span, MatchSpan::new(1, 4)); // bcd
    assert_eq!(result.spans[4].span, MatchSpan::new(1, 5)); // bcde
    assert_eq!(result.spans[5].span, MatchSpan::new(8, 10)); // ef
    assert_eq!(result.spans[6].span, MatchSpan::new(9, 10)); // f
}

/// Estonian multi-byte character offset handling.
#[test]
fn test_estonian_multibyte() {
    let rules = vec![make_substring_rule("öö", HashMap::new(), 0, 0)];
    let tagger = SubstringTagger::new(rules, "", default_config()).unwrap();
    let result = tagger.tag("Tüüpiline öökülma näide");
    assert_eq!(result.spans.len(), 1);
    // char offsets: T(0) ü(1) ü(2) p(3) i(4) l(5) i(6) n(7) e(8) (9)
    //               ö(10) ö(11) k(12) ü(13) l(14) m(15) a(16) (17)
    assert_eq!(result.spans[0].span, MatchSpan::new(10, 12));
}

/// Two rules with same pattern produce ambiguous annotations.
#[test]
fn test_ambiguous_two_rules_same_pattern() {
    let mut a1 = HashMap::new();
    a1.insert(
        "type".to_string(),
        AnnotationValue::Str("capital".to_string()),
    );
    a1.insert(
        "country".to_string(),
        AnnotationValue::Str("US".to_string()),
    );
    let mut a2 = HashMap::new();
    a2.insert(
        "type".to_string(),
        AnnotationValue::Str("name".to_string()),
    );
    a2.insert(
        "country".to_string(),
        AnnotationValue::Str("US".to_string()),
    );

    let rules = vec![
        make_substring_rule("Washington", a1, 0, 0),
        make_substring_rule("Washington", a2, 0, 0),
    ];
    let mut cfg = default_config();
    cfg.output_attributes = vec!["type".to_string(), "country".to_string()];
    let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
    let result = tagger.tag("Washington");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].annotations.len(), 2);
}

/// Pattern attribute injection.
#[test]
fn test_pattern_attribute_injection() {
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

/// Priority-based conflict resolution.
#[test]
fn test_priority_resolution() {
    let mut a1 = HashMap::new();
    a1.insert(
        "type".to_string(),
        AnnotationValue::Str("greeting".to_string()),
    );
    let mut a2 = HashMap::new();
    a2.insert(
        "type".to_string(),
        AnnotationValue::Str("salutation".to_string()),
    );

    // "hello" (priority=0) and "hell" (priority=1) overlap.
    // KEEP_ALL_EXCEPT_PRIORITY should remove priority=1.
    let rules = vec![
        make_substring_rule("hello", a1, 0, 0),
        make_substring_rule("hell", a2, 0, 1),
    ];
    let mut cfg = default_config();
    cfg.conflict_strategy = ConflictStrategy::KeepAllExceptPriority;
    cfg.output_attributes = vec!["type".to_string()];
    let tagger = SubstringTagger::new(rules, "", cfg).unwrap();
    let result = tagger.tag("hello world");
    assert_eq!(result.spans.len(), 1);
    assert_eq!(result.spans[0].span, MatchSpan::new(0, 5)); // hello wins
    assert_eq!(
        result.spans[0].annotations[0].0.get("type"),
        Some(&AnnotationValue::Str("greeting".to_string()))
    );
}
